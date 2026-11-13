//! Window capture through Windows Graphics Capture, the system API that
//! screen recorders use.
//!
//! The system pushes a frame each time the window's content changes. The
//! event handlers only wake a thread this module owns, and that thread makes
//! every Direct3D and capture call. The system's own threads fill the frame
//! pool on the same Direct3D device, so the device is set to serialize its
//! calls. Between reads the thread keeps only the newest frame and copies
//! nothing; when the handler's wait ends, the GPU copies just the values'
//! regions into a small CPU-readable texture, as the crop's layout packs
//! them, and the thread copies those rows into a reused buffer. A frame takes
//! in the window's title bar and borders, so each read first finds where the
//! window's content lies in it, which regions are measured from.
//!
//! A capture that can no longer produce frames ends the way a closed window
//! does: its handler receives a [`io::ErrorKind::NotConnected`] error that
//! says why, and the thread stops. That happens when the Direct3D device is
//! lost, such as after a graphics driver reset or update, and when the frame
//! pool cannot follow the window to a new size. A closed window and a lost
//! device stop frames rather than fail them, so the thread checks every
//! [`HEALTH_CHECK`], whatever arrives meanwhile, that the window is still
//! there and the device still works. The capture item's own Closed event
//! would say sooner that the window closed, but registering a handler for it
//! left a thread handle, events and thread pool waits open after every
//! capture, even once the handler was removed.
//!
//! The thread joins the Windows Runtime's multithreaded apartment for its
//! whole life and leaves it once every object is released, so the system
//! cleans up what the thread's calls set up before the thread ends, instead
//! of the thread borrowing the apartment another thread keeps open.

use std::io;
use std::marker::PhantomData;
use std::ptr;
use std::slice;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureAccess,
    GraphicsCaptureAccessKind, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
use windows::core::{Interface, factory};

use super::{WindowsSystem, client_on_screen, frame_on_screen};
use crate::schedule::{ReadClock, frame_interval};
use crate::screen::{
    Capture, CaptureSession, Crop, Frame, FrameBuffer, FrameHandler, OutsideWindow, Placement,
    Region, Size, content, in_frame, run_capture,
};

/// The frame pool's buffers: one the thread may hold until its next read,
/// and one the system fills meanwhile.
const POOL_BUFFERS: i32 = 2;

/// How long `start` waits for the capture thread to set up.
const START_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the thread checks that its window is there and its Direct3D
/// device works.
const HEALTH_CHECK: Duration = Duration::from_secs(2);

/// Windows time spans count 100-nanosecond ticks.
const TICKS_PER_SECOND: i128 = 10_000_000;

/// The frame handler as [`run_capture`] lends it to the capture's work.
type Handler<'handler> = dyn FnMut(io::Result<Frame<'_>>) -> Duration + 'handler;

/// What wakes the capture thread.
enum Event {
    /// The system has a new frame in the pool.
    Frame,
    /// The session is closing.
    Stop,
}

/// Wakes the capture thread from the system's event handlers and from the
/// session. The channel is unbounded, so no send ever blocks, `Drop`'s
/// included. An extra frame wake costs one look at an empty pool, because
/// the thread takes every frame in the pool when it wakes.
#[derive(Clone)]
struct Wake(Sender<Event>);

impl Wake {
    /// Sends `event` without waiting.
    fn send(&self, event: Event) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a send fails only when the thread has already ended, which is the goal"
        )]
        let _ = self.0.send(event);
    }
}

/// A running Windows capture. Dropping it stops the capture without
/// waiting; [`CaptureSession::close`] also waits for the thread to end.
#[derive(Debug)]
pub struct WindowsCaptureSession {
    events: Wake,
    thread: Option<JoinHandle<()>>,
}

impl CaptureSession for WindowsCaptureSession {
    fn close(mut self) -> io::Result<()> {
        self.events.send(Event::Stop);
        self.thread.take().map_or(Ok(()), |thread| {
            thread
                .join()
                .map_err(|_panic| io::Error::other("the capture thread panicked"))
        })
    }
}

impl Drop for WindowsCaptureSession {
    fn drop(&mut self) {
        self.events.send(Event::Stop);
    }
}

impl std::fmt::Debug for Event {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Frame => "Frame",
            Self::Stop => "Stop",
        })
    }
}

impl std::fmt::Debug for Wake {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Wake")
    }
}

impl Capture for WindowsSystem {
    type Session = WindowsCaptureSession;

    fn start(
        &self,
        window: u64,
        crop: Crop,
        throttle: Duration,
        handler: FrameHandler,
    ) -> io::Result<WindowsCaptureSession> {
        let window = usize::try_from(window)
            .map_err(|_overflow| io::Error::from(io::ErrorKind::InvalidInput))?;
        let (events, received) = mpsc::channel();
        let events = Wake(events);
        let (ready, started) = mpsc::sync_channel(1);
        let wake = events.clone();
        let thread = thread::Builder::new()
            .name("appytappy-capture".into())
            .spawn(move || run(window, crop, throttle, handler, &wake, &received, &ready))?;
        let session = WindowsCaptureSession {
            events,
            thread: Some(thread),
        };
        // On failure, dropping `session` stops the thread without waiting,
        // so the start error is what the caller sees.
        match started.recv_timeout(START_TIMEOUT) {
            Ok(Ok(())) => Ok(session),
            Ok(Err(error)) => Err(error),
            Err(RecvTimeoutError::Timeout) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the capture did not start within ten seconds",
            )),
            Err(RecvTimeoutError::Disconnected) => {
                Err(io::Error::other("the capture thread ended before starting"))
            },
        }
    }
}

/// The capture thread: sets up, reports whether that worked, then handles
/// frames until told to stop.
fn run(
    window: usize,
    crop: Crop,
    throttle: Duration,
    mut handler: FrameHandler,
    wake: &Wake,
    events: &Receiver<Event>,
    ready: &SyncSender<io::Result<()>>,
) {
    let window = HWND(ptr::without_provenance_mut(window));
    // Declared first, so it leaves the apartment after everything else here
    // is dropped.
    let _apartment = match Apartment::join() {
        Ok(apartment) => apartment,
        Err(error) => {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a send fails only when `start` stopped waiting, and nobody is left to tell"
            )]
            let _ = ready.send(Err(error.into()));
            return;
        },
    };
    let mut capture = match Capturer::start(window, throttle, wake) {
        Ok(capture) => capture,
        Err(error) => {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a send fails only when `start` stopped waiting, and nobody is left to tell"
            )]
            let _ = ready.send(Err(error.into()));
            return;
        },
    };
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a send fails only when `start` stopped waiting, and nobody is left to tell"
    )]
    let _ = ready.send(Ok(()));
    // A panic while it handles frames reaches the handler as the capture's
    // end, and `stop` still releases what the capture holds.
    run_capture(&mut handler, |handler| {
        capture.handle_frames(&crop, handler, events);
    });
    capture.stop();
}

/// The capture thread's membership in the Windows Runtime's multithreaded
/// apartment. It ends on drop, on the thread that joined, because the type
/// cannot move between threads.
struct Apartment(PhantomData<*const ()>);

impl Apartment {
    fn join() -> windows::core::Result<Self> {
        #[expect(unsafe_code, reason = "a Windows Runtime call")]
        // SAFETY: the call takes a flag; a success, including S_FALSE for a
        // thread that already joined, is balanced by RoUninitialize on drop.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }?;
        Ok(Self(PhantomData))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "a Windows Runtime call")]
        // SAFETY: this value's successful RoInitialize ran on this thread,
        // since the type is not Send, and the capture's objects are released
        // before it, because `run` declares it first.
        unsafe {
            RoUninitialize();
        };
    }
}

/// A frame held until the handler's wait ends.
struct Held {
    frame: Direct3D11CaptureFrame,
    received: Instant,
    delay: Duration,
}

/// Every Direct3D and capture object of one session, owned by its thread.
struct Capturer {
    window: HWND,
    /// The process that owned the window when the capture started, so a
    /// window handle that Windows reused for another process's window reads
    /// as closed.
    owner: u32,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt_device: IDirect3DDevice,
    pool: Direct3D11CaptureFramePool,
    pool_size: SizeInt32,
    session: GraphicsCaptureSession,
    arrived: i64,
    /// The least time between two frames that the session may ask for.
    throttle: Duration,
    /// The time between frames that the session asks the system for now.
    asked: Duration,
    staging: Option<(Size, ID3D11Texture2D)>,
    buffer: FrameBuffer,
    ticks_per_second: i128,
}

impl Capturer {
    fn start(window: HWND, throttle: Duration, wake: &Wake) -> windows::core::Result<Self> {
        // First, so a window that is already gone fails before anything is
        // made, and one whose handle another window takes meanwhile reads
        // as closed at the first check.
        let owner = window_owner(window).ok_or_else(windows::core::Error::from_thread)?;
        let mut frequency = 0;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `frequency` is a live i64 the call writes.
        unsafe { QueryPerformanceFrequency(&raw mut frequency) }?;
        let (device, context) = create_device()?;
        let multithread: ID3D11Multithread = device.cast()?;
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the result is the previous setting, which nothing needs"
        )]
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes a flag; `multithread` is live.
        let _ = unsafe { multithread.SetMultithreadProtected(true) };
        let dxgi: IDXGIDevice = device.cast()?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `dxgi` is a live DXGI device; the call returns a new reference.
        let winrt_device: IDirect3DDevice =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }?.cast()?;
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes a window handle by value; an invalid handle
        // returns an error rather than reading memory.
        let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(window) }?;
        let pool_size = item.Size()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            POOL_BUFFERS,
            pool_size,
        )?;
        // From here on, a failure releases what the pool holds as `stop`
        // does.
        let session = pool
            .CreateCaptureSession(&item)
            .inspect_err(|_| release(&pool, None, None, &context, &winrt_device))?;
        let frames = wake.clone();
        let arrived = pool
            .FrameArrived(&TypedEventHandler::new(move |_, _| {
                frames.send(Event::Frame);
                Ok(())
            }))
            .inspect_err(|_| release(&pool, None, Some(&session), &context, &winrt_device))?;
        configure(&session);
        ask_interval(&session, throttle);
        session.StartCapture().inspect_err(|_| {
            release(
                &pool,
                Some(arrived),
                Some(&session),
                &context,
                &winrt_device,
            );
        })?;
        Ok(Self {
            window,
            owner,
            device,
            context,
            winrt_device,
            pool,
            pool_size,
            session,
            arrived,
            throttle,
            asked: throttle,
            staging: None,
            buffer: FrameBuffer::default(),
            ticks_per_second: i128::from(frequency).max(1),
        })
    }

    /// Waits for frames and hands the newest to `handler` whenever its last
    /// wait has ended, until the session stops or the capture ends; an
    /// ending reaches the handler as a [`io::ErrorKind::NotConnected`] error.
    fn handle_frames(&mut self, crop: &Crop, handler: &mut Handler<'_>, events: &Receiver<Event>) {
        let mut held: Option<Held> = None;
        let mut clock = ReadClock::new(Instant::now());
        let mut checked = Instant::now();
        loop {
            let now = Instant::now();
            let check_in = HEALTH_CHECK.saturating_sub(now.saturating_duration_since(checked));
            let wait = match held {
                Some(_) => clock.remaining(now).min(check_in),
                None => check_in,
            };
            match events.recv_timeout(wait) {
                Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Event::Frame) => self.take_newest(&mut held),
                Err(RecvTimeoutError::Timeout) => {},
            }
            // Checked on a deadline, so neither steady wakes nor a handler
            // that waits long put the check off.
            let now = Instant::now();
            if now.saturating_duration_since(checked) >= HEALTH_CHECK {
                checked = now;
                if let Some(ending) = self.closed_window().or_else(|| self.lost_device()) {
                    end(handler, ending);
                    break;
                }
            }
            if let Some(frame) = held.take_if(|_| clock.due(Instant::now())) {
                let Some(wait) = self.read(frame, crop, handler) else {
                    break;
                };
                clock.after_read(Instant::now(), wait);
                self.follow(wait);
            }
        }
        if let Some(frame) = held {
            close(&frame.frame);
        }
    }

    /// Asks the system for frames no more often than the handler reads
    /// them, as [`frame_interval`] works out, so a window whose values hold
    /// still costs the system fewer copies too. The system is expected to
    /// hold back a change drawn inside that time and deliver it once the time
    /// has passed, which is when the handler would have read again anyway;
    /// the footprint check's settle check tests that on Windows.
    fn follow(&mut self, wait: Duration) {
        let interval = frame_interval(wait, self.throttle);
        if interval != self.asked {
            self.asked = interval;
            ask_interval(&self.session, interval);
        }
    }

    /// Takes every frame the pool holds, keeping only the newest.
    fn take_newest(&self, held: &mut Option<Held>) {
        while let Ok(frame) = self.pool.TryGetNextFrame() {
            let received = Instant::now();
            let delay = self.delay(&frame).unwrap_or_default();
            if let Some(older) = held.replace(Held {
                frame,
                received,
                delay,
            }) {
                close(&older.frame);
            }
        }
    }

    /// How long ago the system composed `frame`.
    fn delay(&self, frame: &Direct3D11CaptureFrame) -> Option<Duration> {
        let composed = i128::from(frame.SystemRelativeTime().ok()?.Duration);
        let mut counter = 0;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `counter` is a live i64 the call writes.
        unsafe { QueryPerformanceCounter(&raw mut counter) }.ok()?;
        let now = i128::from(counter)
            .checked_mul(TICKS_PER_SECOND)?
            .checked_div(self.ticks_per_second)?;
        let ticks = u64::try_from(now.checked_sub(composed)?).ok()?;
        Some(Duration::from_nanos(ticks.saturating_mul(100)))
    }

    /// Copies `crop` of one frame and passes it to `handler`, returning the
    /// handler's wait, or `None` when the capture ended.
    fn read(&mut self, held: Held, crop: &Crop, handler: &mut Handler<'_>) -> Option<Duration> {
        let copied = self.copy(&held.frame, crop);
        close(&held.frame);
        match copied {
            Ok(Some(window_size)) => Some(match self.buffer.image() {
                Some(image) => handler(Ok(Frame {
                    window_size,
                    image,
                    received: held.received,
                    delay: held.delay,
                })),
                None => Duration::ZERO,
            }),
            // The pool was resized for the window's new size; the next frame
            // has the right size.
            Ok(None) => Some(Duration::ZERO),
            Err(error) => match self.lost_device() {
                Some(lost) => {
                    end(handler, lost);
                    None
                },
                None if error.kind() == io::ErrorKind::NotConnected => {
                    end(handler, error);
                    None
                },
                None => Some(handler(Err(error))),
            },
        }
    }

    /// Why the capture cannot go on because its window closed, or `None`
    /// while the window is there.
    fn closed_window(&self) -> Option<io::Error> {
        (window_owner(self.window) != Some(self.owner)).then(|| ended("the captured window closed"))
    }

    /// Why the Direct3D device can no longer be used, such as after a
    /// graphics driver reset, or `None` while it works.
    fn lost_device(&self) -> Option<io::Error> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes no arguments and only reads the state of
        // `device`, which is live.
        let removed = unsafe { self.device.GetDeviceRemovedReason() };
        removed
            .err()
            .map(|error| ended(&format!("the graphics device was lost: {error}")))
    }

    /// Where the window's content lies in a frame of `captured` size, or
    /// `None` when its client area cannot be read or covers none of the
    /// frame, and the whole frame is the content.
    fn content(&self, captured: Size) -> Option<Region> {
        let frame = frame_on_screen(self.window).ok()?;
        let client = client_on_screen(self.window).ok()?;
        content(frame, client, captured)
    }

    /// Copies `crop` of `frame`'s content into the buffer and returns the
    /// content's size, or `None` when the frame had to be dropped for a
    /// resize.
    fn copy(&mut self, frame: &Direct3D11CaptureFrame, crop: &Crop) -> io::Result<Option<Size>> {
        let surface = frame.ContentSize()?;
        if surface != self.pool_size {
            self.pool
                .Recreate(
                    &self.winrt_device,
                    DirectXPixelFormat::B8G8R8A8UIntNormalized,
                    POOL_BUFFERS,
                    surface,
                )
                .map_err(|error| {
                    ended(&format!(
                        "the capture cannot follow the window's new size: {error}"
                    ))
                })?;
            self.pool_size = surface;
            return Ok(None);
        }
        let captured = Size {
            width: u32::try_from(surface.Width).unwrap_or_default(),
            height: u32::try_from(surface.Height).unwrap_or_default(),
        };
        let area = self
            .content(captured)
            .or_else(|| Region::whole(captured))
            .ok_or(OutsideWindow {
                window_size: captured,
            })?;
        let window_size = area.size();
        let outside = OutsideWindow { window_size };
        let whole = [Placement {
            from: Region::whole(window_size).ok_or(outside)?,
            y: 0,
        }];
        let (size, placements) = match crop {
            Crop::Whole => (window_size, whole.as_slice()),
            Crop::Layout(layout) if layout.fits(window_size) => (layout.size(), layout.copies()),
            Crop::Layout(_) => return Err(outside.into()),
        };
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `access` wraps the frame's live surface; the call returns a
        // new reference to its texture.
        let source: ID3D11Texture2D = unsafe { access.GetInterface() }?;
        let staging = self.staging(size)?;
        for placement in placements {
            let from = in_frame(placement.from, area).ok_or(outside)?;
            let copied = D3D11_BOX {
                left: from.x(),
                top: from.y(),
                front: 0,
                right: from.right(),
                bottom: from.bottom(),
                back: 1,
            };
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: both textures are live and BGRA; `copied` lies inside
            // the window's content, which `in_frame` checked, and the content
            // lies inside `captured`, which `content` makes sure of; the
            // source has the size `captured`, since a frame of another size
            // than the pool's recreated the pool above instead. The layout
            // placed the copy inside the staging texture, which has the
            // layout's size.
            unsafe {
                self.context.CopySubresourceRegion(
                    &staging,
                    0,
                    0,
                    placement.y,
                    0,
                    &source,
                    0,
                    Some(&raw const copied),
                );
            };
        }
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `staging` is a live CPU-readable texture and `mapped` a live
        // struct the call writes.
        unsafe {
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&raw mut mapped))
        }?;
        let copied = self.copy_rows(&mapped, size);
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `staging` was mapped above and is unmapped once.
        unsafe {
            self.context.Unmap(&staging, 0);
        };
        copied.map(|()| Some(window_size))
    }

    /// Copies a mapped texture's rows into the buffer.
    fn copy_rows(&mut self, mapped: &D3D11_MAPPED_SUBRESOURCE, size: Size) -> io::Result<()> {
        let invalid = |error| io::Error::new(io::ErrorKind::InvalidData, error);
        let pitch = usize::try_from(mapped.RowPitch).map_err(io::Error::other)?;
        let row = usize::try_from(size.width)
            .ok()
            .and_then(|width| width.checked_mul(4));
        let length = usize::try_from(size.height)
            .ok()
            .and_then(|height| height.checked_sub(1))
            .and_then(|rows| rows.checked_mul(pitch))
            .zip(row)
            .and_then(|(start, row)| start.checked_add(row))
            .ok_or_else(|| io::Error::other("the mapped texture is too large"))?;
        if mapped.pData.is_null() || row.is_none_or(|row| pitch < row) {
            return Err(io::Error::other(
                "the mapped texture is smaller than its region",
            ));
        }
        #[expect(unsafe_code, reason = "reads a mapped Direct3D texture")]
        // SAFETY: Direct3D mapped `size.height` rows `pitch` bytes apart at
        // `pData`, each at least `row` bytes long since `pitch >= row`, so
        // the last row ends `length` bytes in; the mapping stays valid until
        // `Unmap`, after this slice is gone.
        let source =
            unsafe { slice::from_raw_parts(mapped.pData.cast::<u8>().cast_const(), length) };
        self.buffer
            .fill_pitched(size, source, pitch)
            .map_err(invalid)
    }

    /// The staging texture for `size`, made once and remade only when the
    /// packed image's size changes.
    fn staging(&mut self, size: Size) -> windows::core::Result<ID3D11Texture2D> {
        if let Some((cached, texture)) = &self.staging
            && *cached == size
        {
            return Ok(texture.clone());
        }
        let description = D3D11_TEXTURE2D_DESC {
            Width: size.width,
            Height: size.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0.cast_unsigned(),
            MiscFlags: 0,
        };
        let mut texture = None;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `description` and `texture` are live; the call writes a new
        // reference into `texture`.
        unsafe {
            self.device
                .CreateTexture2D(&raw const description, None, Some(&raw mut texture))
        }?;
        let texture = texture.ok_or_else(windows::core::Error::empty)?;
        self.staging = Some((size, texture.clone()));
        Ok(texture)
    }

    /// Ends the capture and releases the system's objects, as [`release`]
    /// says.
    fn stop(self) {
        drop(self.staging);
        release(
            &self.pool,
            Some(self.arrived),
            Some(&self.session),
            &self.context,
            &self.winrt_device,
        );
    }
}

/// Ends a capture, or one that failed to start, and releases what its frame
/// pool holds: it removes the frame handler and closes the session and the
/// pool. Direct3D destroys released objects only when its context next
/// flushes, so the context then drops what it holds and flushes. Closing the
/// Direct3D device's Windows Runtime wrapper releases the device it holds
/// even if the system still holds the wrapper.
fn release(
    pool: &Direct3D11CaptureFramePool,
    arrived: Option<i64>,
    session: Option<&GraphicsCaptureSession>,
    context: &ID3D11DeviceContext,
    winrt_device: &IDirect3DDevice,
) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the capture is ending, and a failure to unregister or close leaves nothing to retry"
    )]
    let _ = (
        arrived.map(|token| pool.RemoveFrameArrived(token)),
        session.map(GraphicsCaptureSession::Close),
        pool.Close(),
    );
    #[expect(unsafe_code, reason = "Win32 calls")]
    // SAFETY: both calls take no arguments; `context` is live, and only the
    // capture's own thread uses it.
    unsafe {
        context.ClearState();
        context.Flush();
    };
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the capture is ending, and a failure to close leaves nothing to retry"
    )]
    let _ = winrt_device.Close();
}

/// Asks for a borderless capture without the cursor. Each setting is
/// optional: older Windows versions lack some, and the capture works without
/// them.
fn configure(session: &GraphicsCaptureSession) {
    if let Ok(request) =
        GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)
    {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a refusal only keeps the yellow capture border"
        )]
        let _ = request.join();
    }
    #[expect(
        clippy::let_underscore_must_use,
        reason = "each setting is optional and missing on older Windows versions"
    )]
    let _ = (
        session.SetIsBorderRequired(false),
        session.SetIsCursorCaptureEnabled(false),
    );
}

/// Asks the system for at most one frame every `interval`. Older Windows
/// versions lack the setting, and then frames come as often as the window
/// changes.
fn ask_interval(session: &GraphicsCaptureSession, interval: Duration) {
    let ticks = i64::try_from(interval.as_nanos() / 100).unwrap_or(i64::MAX);
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the setting is optional and missing on older Windows versions"
    )]
    let _ = session.SetMinUpdateInterval(TimeSpan { Duration: ticks });
}

/// The ID of the process that owns `window`, or `None` when no window has
/// that handle.
fn window_owner(window: HWND) -> Option<u32> {
    let mut process = 0_u32;
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes a window handle by value and writes `process`,
    // a live local; a handle that names no window returns 0 and writes
    // nothing.
    let thread = unsafe { GetWindowThreadProcessId(window, Some(&raw mut process)) };
    (thread != 0).then_some(process)
}

/// The error that tells a handler its capture ended.
fn ended(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, why)
}

/// Tells `handler` that its capture ended. Its wait is moot: nothing more
/// will arrive.
fn end(handler: &mut Handler<'_>, error: io::Error) {
    let _wait = handler(Err(error));
}

/// Returns a frame's buffer to the pool.
fn close(frame: &Direct3D11CaptureFrame) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a frame that fails to close is released when its last reference drops"
    )]
    let _ = frame.Close();
}

/// A Direct3D device on the GPU, or on Windows' software renderer when no
/// GPU device is available.
fn create_device() -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
    create_device_of(D3D_DRIVER_TYPE_HARDWARE).or_else(|_| create_device_of(D3D_DRIVER_TYPE_WARP))
}

fn create_device_of(
    driver: D3D_DRIVER_TYPE,
) -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None;
    let mut context = None;
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `device` and `context` are live options the call writes.
    unsafe {
        D3D11CreateDevice(
            None,
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&raw mut device),
            None,
            Some(&raw mut context),
        )
    }?;
    device.zip(context).ok_or_else(windows::core::Error::empty)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::screen::{Image, Layout, Region};
    use crate::system::System;
    use crate::win::window_handle;

    /// Captures a visible window on the desktop of whoever runs it. It is
    /// ignored because CI runners have no visible windows.
    #[test]
    #[ignore = "needs a visible window on an interactive desktop"]
    fn captures_a_visible_window() {
        let system = WindowsSystem::new();
        let window = system
            .windows()
            .unwrap()
            .into_iter()
            .find(|window| {
                window.visible
                    && window.bounds.right - window.bounds.left >= 64
                    && window.bounds.bottom - window.bounds.top >= 64
                    && !window.title.is_empty()
            })
            .expect("a visible window of at least 64 by 64 pixels");
        let (frames, received) = mpsc::channel();
        let session = system
            .start(
                window.handle,
                Crop::Layout(Layout::one(Region::new(0, 0, 32, 16).unwrap())),
                Duration::ZERO,
                Box::new(move |frame| {
                    let frame = frame.unwrap();
                    frames.send((frame.image.size(), frame.latency())).unwrap();
                    Duration::from_millis(10)
                }),
            )
            .unwrap();
        let (size, latency) = received
            .recv_timeout(Duration::from_secs(5))
            .expect("a frame within five seconds");
        session.close().unwrap();
        assert_eq!(
            size,
            Size {
                width: 32,
                height: 16
            }
        );
        assert!(latency < Duration::from_secs(5), "{latency:?}");
    }

    /// The newest frame's window size and image for `crop` of `window`.
    fn capture_once(system: &WindowsSystem, window: u64, crop: Crop) -> (Size, Image) {
        let (frames, received) = mpsc::channel();
        let session = system
            .start(
                window,
                crop,
                Duration::ZERO,
                Box::new(move |frame| {
                    let frame = frame.unwrap();
                    // The test stops listening after the first frame.
                    let _sent = frames.send((frame.window_size, frame.image.clone()));
                    Duration::from_secs(60)
                }),
            )
            .unwrap();
        let frame = received
            .recv_timeout(Duration::from_secs(5))
            .expect("a frame within five seconds");
        session.close().unwrap();
        frame
    }

    /// Packs two far-apart regions of a visible window and checks that each
    /// lands where the layout says, by comparing it with the same region of
    /// a whole frame. The window may change between the two captures, so it
    /// tries a few times. It is ignored because CI runners have no visible
    /// windows.
    #[test]
    #[ignore = "needs a visible window on an interactive desktop"]
    fn packs_each_region_where_its_layout_says() {
        let system = WindowsSystem::new();
        let window = system
            .windows()
            .unwrap()
            .into_iter()
            .find(|window| {
                window.visible
                    && window.bounds.right - window.bounds.left >= 256
                    && window.bounds.bottom - window.bounds.top >= 256
                    && !window.title.is_empty()
            })
            .expect("a visible window of at least 256 by 256 pixels");
        let (size, _) = capture_once(&system, window.handle, Crop::Whole);
        let regions = [
            Region::new(4, 4, 40, 12).unwrap(),
            Region::new(size.width - 36, size.height - 28, 32, 24).unwrap(),
        ];
        let layout = Layout::new(regions.map(Some)).unwrap();
        assert_eq!(layout.copies().len(), 2, "far apart regions are stacked");
        let matched = (0..5).any(|_| {
            let (_, packed) = capture_once(&system, window.handle, Crop::Layout(layout.clone()));
            let (_, whole) = capture_once(&system, window.handle, Crop::Whole);
            assert_eq!(packed.size(), layout.size());
            regions.iter().enumerate().all(|(slot, region)| {
                packed.crop(layout.packed(slot).unwrap()) == whole.crop(*region)
            })
        });
        assert!(matched, "each packed region matches the whole frame's");
    }

    /// Captures a visible window with a title bar whole and checks that the
    /// frame holds its content alone, at the size the system says the
    /// window's content has. It is ignored because CI runners have no
    /// visible windows.
    #[test]
    #[ignore = "needs a visible window with a title bar on an interactive desktop"]
    fn a_frame_holds_the_window_s_content_alone() {
        let system = WindowsSystem::new();
        let window = system
            .windows()
            .unwrap()
            .into_iter()
            .find(|window| {
                let content = system.content_bounds(window.handle).ok().flatten();
                let frame = frame_on_screen(window_handle(window.handle).unwrap());
                window.visible
                    && !window.title.is_empty()
                    && content.zip(frame.ok()).is_some_and(|(content, frame)| {
                        content.bottom - content.top >= 64 && content.top > frame.top
                    })
            })
            .expect("a visible window with a title bar");
        let (size, image) = capture_once(&system, window.handle, Crop::Whole);
        assert_eq!(Some(size), system.content_size(window.handle).unwrap());
        assert_eq!(image.size(), size);
    }

    #[test]
    fn refuses_a_window_that_does_not_exist() {
        let system = WindowsSystem::new();
        let started = system.start(0, Crop::Whole, Duration::ZERO, Box::new(|_| Duration::ZERO));
        assert!(started.is_err());
    }
}
