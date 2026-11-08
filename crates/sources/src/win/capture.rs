//! Window capture through Windows Graphics Capture, the system API that
//! screen recorders use.
//!
//! The system pushes a frame each time the window's content changes. The
//! event handlers only wake a thread this module owns, and that thread makes
//! every Direct3D and capture call. The system's own threads fill the frame
//! pool on the same Direct3D device, so the device is set to serialize its
//! calls. Between reads the thread keeps only the newest frame and copies
//! nothing; when the handler's wait ends, the GPU copies just the region into
//! a small CPU-readable texture, and the thread copies those rows into a
//! reused buffer.

use std::io;
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
use windows::core::{Interface, factory};

use super::WindowsSystem;
use crate::schedule::ReadClock;
use crate::screen::{
    Capture, CaptureSession, Crop, Frame, FrameBuffer, FrameHandler, OutsideWindow, Region, Size,
};

/// The frame pool's buffers: one the thread may hold until its next read,
/// and one the system fills meanwhile.
const POOL_BUFFERS: i32 = 2;

/// How long `start` waits for the capture thread to set up.
const START_TIMEOUT: Duration = Duration::from_secs(10);

/// Windows time spans count 100-nanosecond ticks.
const TICKS_PER_SECOND: i128 = 10_000_000;

/// What wakes the capture thread.
enum Event {
    /// The system has a new frame in the pool.
    Frame,
    /// The window closed, so no more frames will come.
    Closed,
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
            Self::Closed => "Closed",
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
    capture.handle_frames(crop, &mut handler, events);
    capture.stop();
}

/// A frame held until the handler's wait ends.
struct Held {
    frame: Direct3D11CaptureFrame,
    received: Instant,
    delay: Duration,
}

/// Every Direct3D and capture object of one session, owned by its thread.
struct Capturer {
    item: GraphicsCaptureItem,
    closed: i64,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt_device: IDirect3DDevice,
    pool: Direct3D11CaptureFramePool,
    pool_size: SizeInt32,
    session: GraphicsCaptureSession,
    arrived: i64,
    staging: Option<(Size, ID3D11Texture2D)>,
    buffer: FrameBuffer,
    ticks_per_second: i128,
}

impl Capturer {
    fn start(window: HWND, throttle: Duration, wake: &Wake) -> windows::core::Result<Self> {
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
        let frames = wake.clone();
        let arrived = pool.FrameArrived(&TypedEventHandler::new(move |_, _| {
            frames.send(Event::Frame);
            Ok(())
        }))?;
        let ended = wake.clone();
        let closed = item.Closed(&TypedEventHandler::new(move |_, _| {
            ended.send(Event::Closed);
            Ok(())
        }))?;
        let session = pool.CreateCaptureSession(&item)?;
        configure(&session, throttle);
        session.StartCapture()?;
        let mut frequency = 0;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `frequency` is a live i64 the call writes.
        unsafe { QueryPerformanceFrequency(&raw mut frequency) }?;
        Ok(Self {
            item,
            closed,
            device,
            context,
            winrt_device,
            pool,
            pool_size,
            session,
            arrived,
            staging: None,
            buffer: FrameBuffer::default(),
            ticks_per_second: i128::from(frequency).max(1),
        })
    }

    /// Waits for frames and hands the newest to `handler` whenever its last
    /// wait has ended, until the session stops or the window closes; a
    /// closed window reaches the handler as a [`io::ErrorKind::NotConnected`]
    /// error.
    fn handle_frames(&mut self, crop: Crop, handler: &mut FrameHandler, events: &Receiver<Event>) {
        let mut held: Option<Held> = None;
        let mut clock = ReadClock::new(Instant::now());
        loop {
            let event = if held.is_some() {
                events.recv_timeout(clock.remaining(Instant::now()))
            } else {
                events
                    .recv()
                    .map_err(|_disconnected| RecvTimeoutError::Disconnected)
            };
            match event {
                Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Event::Closed) => {
                    // The handler's wait is moot: nothing more will arrive.
                    let _wait = handler(Err(io::Error::new(
                        io::ErrorKind::NotConnected,
                        "the captured window closed",
                    )));
                    break;
                },
                Ok(Event::Frame) => self.take_newest(&mut held),
                Err(RecvTimeoutError::Timeout) => {},
            }
            if let Some(frame) = held.take_if(|_| clock.due(Instant::now())) {
                let wait = self.read(frame, crop, handler);
                clock.after_read(Instant::now(), wait);
            }
        }
        if let Some(frame) = held {
            close(&frame.frame);
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
    /// handler's wait.
    fn read(&mut self, held: Held, crop: Crop, handler: &mut FrameHandler) -> Duration {
        let copied = self.copy(&held.frame, crop);
        close(&held.frame);
        match copied {
            Ok(Some(window_size)) => match self.buffer.image() {
                Some(image) => handler(Ok(Frame {
                    window_size,
                    image,
                    received: held.received,
                    delay: held.delay,
                })),
                None => Duration::ZERO,
            },
            // The pool was resized for the window's new size; the next frame
            // has the right size.
            Ok(None) => Duration::ZERO,
            Err(error) => handler(Err(error)),
        }
    }

    /// Copies `crop` of `frame` into the buffer and returns the window's
    /// size, or `None` when the frame had to be dropped for a resize.
    fn copy(&mut self, frame: &Direct3D11CaptureFrame, crop: Crop) -> io::Result<Option<Size>> {
        let content = frame.ContentSize()?;
        if content != self.pool_size {
            self.pool.Recreate(
                &self.winrt_device,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                POOL_BUFFERS,
                content,
            )?;
            self.pool_size = content;
            return Ok(None);
        }
        let window_size = Size {
            width: u32::try_from(content.Width).unwrap_or_default(),
            height: u32::try_from(content.Height).unwrap_or_default(),
        };
        let region = match crop {
            Crop::Whole => Region::whole(window_size),
            Crop::Region(region) => Some(region),
        }
        .filter(|region| region.fits(window_size))
        .ok_or(OutsideWindow { window_size })?;
        let size = region.size();
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `access` wraps the frame's live surface; the call returns a
        // new reference to its texture.
        let source: ID3D11Texture2D = unsafe { access.GetInterface() }?;
        let staging = self.staging(size)?;
        let area = D3D11_BOX {
            left: region.x(),
            top: region.y(),
            front: 0,
            right: region.right(),
            bottom: region.bottom(),
            back: 1,
        };
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: both textures are live and BGRA; `area` lies inside the
        // source, which `fits` checked, and matches the staging texture's size.
        unsafe {
            self.context.CopySubresourceRegion(
                &staging,
                0,
                0,
                0,
                0,
                &source,
                0,
                Some(&raw const area),
            );
        };
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
    /// region's size changes.
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

    /// Ends the capture and releases the system's objects.
    fn stop(self) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the session is ending, and a failure to unregister or close leaves nothing to retry"
        )]
        let _ = (
            self.pool.RemoveFrameArrived(self.arrived),
            self.item.RemoveClosed(self.closed),
            self.session.Close(),
            self.pool.Close(),
        );
    }
}

/// Asks for a borderless capture without the cursor, at most one frame per
/// `throttle`. Each is optional: older Windows versions lack some, and the
/// capture works without them.
fn configure(session: &GraphicsCaptureSession, throttle: Duration) {
    if let Ok(request) =
        GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)
    {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a refusal only keeps the yellow capture border"
        )]
        let _ = request.join();
    }
    let ticks = i64::try_from(throttle.as_nanos() / 100).unwrap_or(i64::MAX);
    #[expect(
        clippy::let_underscore_must_use,
        reason = "each setting is optional and missing on older Windows versions"
    )]
    let _ = (
        session.SetIsBorderRequired(false),
        session.SetIsCursorCaptureEnabled(false),
        session.SetMinUpdateInterval(TimeSpan { Duration: ticks }),
    );
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
    use crate::screen::Region;
    use crate::system::System;

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
                Crop::Region(Region::new(0, 0, 32, 16).unwrap()),
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

    #[test]
    fn refuses_a_window_that_does_not_exist() {
        let system = WindowsSystem::new();
        let started = system.start(0, Crop::Whole, Duration::ZERO, Box::new(|_| Duration::ZERO));
        assert!(started.is_err());
    }
}
