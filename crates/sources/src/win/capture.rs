//! Window capture through Windows Graphics Capture, the system API that
//! screen recorders use.
//!
//! The system pushes a frame each time the window's content changes. The
//! event handlers only wake a thread this module owns, and that thread makes
//! every Direct3D and capture call. The system's own threads fill the frame
//! pool on the same Direct3D device, so the device is set to serialize its
//! calls.
//!
//! [`run_frames`] decides when a frame is read, which one, and what ends the
//! capture; this module only makes the calls it asks for, as its
//! [`FrameSource`]. To read a frame, the GPU copies just the values' regions
//! into a small CPU-readable texture, as the crop's layout packs them, and
//! the thread copies those rows into a reused buffer.
//!
//! A closed window and a lost Direct3D device stop frames rather than fail
//! them, so the loop asks about both on a deadline. The capture item's own
//! Closed event would say sooner that the window closed, but registering a
//! handler for it left a thread handle, events and thread pool waits open
//! after every capture, even once the handler was removed.
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
use crate::screen::{
    Capture, CaptureSession, Crop, FrameBuffer, FrameHandler, FrameSource, Placement, Size, Wake,
    composed_ago, run_capture, run_frames,
};
use crate::system::Bounds;

/// The frame pool's buffers: one the thread may hold until its next read,
/// and one the system fills meanwhile.
const POOL_BUFFERS: i32 = 2;

/// How long `start` waits for the capture thread to set up.
const START_TIMEOUT: Duration = Duration::from_secs(10);

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
struct Waker(Sender<Event>);

impl Waker {
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
    events: Waker,
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

impl std::fmt::Debug for Waker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Waker")
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
        let events = Waker(events);
        let (ready, started) = mpsc::sync_channel(1);
        let wake = events.clone();
        let thread = thread::Builder::new()
            .name("appytappy-capture".into())
            .spawn(move || run(window, &crop, throttle, handler, &wake, received, &ready))?;
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
    crop: &Crop,
    throttle: Duration,
    mut handler: FrameHandler,
    wake: &Waker,
    events: Receiver<Event>,
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
    let (mut capture, frames) = match Capturer::start(window, throttle, wake, events) {
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
        run_frames(&mut capture, frames, throttle, crop, handler);
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
    session: GraphicsCaptureSession,
    arrived: i64,
    staging: Option<(Size, ID3D11Texture2D)>,
    /// How many times a second the performance counter counts.
    frequency: i64,
    /// What wakes the thread.
    events: Receiver<Event>,
}

impl Capturer {
    /// Starts capturing `window`, asking for at most one frame every
    /// `throttle`, and returns the capture with the size of its frames.
    fn start(
        window: HWND,
        throttle: Duration,
        wake: &Waker,
        events: Receiver<Event>,
    ) -> windows::core::Result<(Self, Size)> {
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
        let capture = Self {
            window,
            owner,
            device,
            context,
            winrt_device,
            pool,
            session,
            arrived,
            staging: None,
            frequency,
            events,
        };
        Ok((capture, size_of(pool_size)))
    }

    /// How long ago the system composed `frame`.
    fn delay(&self, frame: &Direct3D11CaptureFrame) -> Option<Duration> {
        let composed = frame.SystemRelativeTime().ok()?.Duration;
        let mut counter = 0;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `counter` is a live i64 the call writes.
        unsafe { QueryPerformanceCounter(&raw mut counter) }.ok()?;
        composed_ago(composed, counter, self.frequency)
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

impl FrameSource for Capturer {
    type Frame = Direct3D11CaptureFrame;

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn wait(&mut self, timeout: Duration) -> Wake {
        match self.events.recv_timeout(timeout) {
            Ok(Event::Frame) => Wake::Frame,
            Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => Wake::Stop,
            Err(RecvTimeoutError::Timeout) => Wake::Timeout,
        }
    }

    fn next_frame(&mut self) -> Option<(Direct3D11CaptureFrame, Duration)> {
        let frame = self.pool.TryGetNextFrame().ok()?;
        let delay = self.delay(&frame).unwrap_or_default();
        Some((frame, delay))
    }

    fn release(&mut self, frame: Direct3D11CaptureFrame) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a frame that fails to close is released when its last reference drops"
        )]
        let _ = frame.Close();
    }

    /// A window handle that names no window, or one that Windows reused for
    /// another process's window, is a closed window.
    fn window_closed(&self) -> bool {
        window_owner(self.window) != Some(self.owner)
    }

    fn lost_device(&self) -> Option<String> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes no arguments and only reads the state of
        // `device`, which is live.
        let removed = unsafe { self.device.GetDeviceRemovedReason() };
        removed.err().map(|error| error.to_string())
    }

    fn frame_size(&self, frame: &Direct3D11CaptureFrame) -> io::Result<Size> {
        Ok(size_of(frame.ContentSize()?))
    }

    fn resize_frames(&mut self, size: Size) -> Result<(), String> {
        let length = |pixels: u32| i32::try_from(pixels).map_err(|error| error.to_string());
        self.pool
            .Recreate(
                &self.winrt_device,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                POOL_BUFFERS,
                SizeInt32 {
                    Width: length(size.width)?,
                    Height: length(size.height)?,
                },
            )
            .map_err(|error| error.to_string())
    }

    fn window_bounds(&self) -> Option<(Bounds, Bounds)> {
        let frame = frame_on_screen(self.window).ok()?;
        let client = client_on_screen(self.window).ok()?;
        Some((frame, client))
    }

    fn copy(
        &mut self,
        frame: &Direct3D11CaptureFrame,
        size: Size,
        copies: &[Placement],
        buffer: &mut FrameBuffer,
    ) -> io::Result<()> {
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `access` wraps the frame's live surface; the call returns a
        // new reference to its texture.
        let source: ID3D11Texture2D = unsafe { access.GetInterface() }?;
        let staging = self.staging(size)?;
        for placement in copies {
            let from = placement.from;
            let source_box = D3D11_BOX {
                left: from.x(),
                top: from.y(),
                front: 0,
                right: from.right(),
                bottom: from.bottom(),
                back: 1,
            };
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: both textures are live and BGRA, and `source_box` is a
            // live local the call reads. The box lies inside the source,
            // and the copy lands inside the staging texture, which has
            // `size`: `run_frames` passes only copies that lie inside the
            // frame's picture and land inside `size`, and reads a frame
            // only when its picture has the size of the pool's textures.
            unsafe {
                self.context.CopySubresourceRegion(
                    &staging,
                    0,
                    0,
                    placement.y,
                    0,
                    &source,
                    0,
                    Some(&raw const source_box),
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
        let filled = copy_rows(&mapped, size, buffer);
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `staging` was mapped above and is unmapped once.
        unsafe {
            self.context.Unmap(&staging, 0);
        };
        filled
    }

    fn ask_interval(&mut self, interval: Duration) {
        ask_interval(&self.session, interval);
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

/// `size` as the sizes of pictures are counted; a side the system gives as
/// negative counts as none.
fn size_of(size: SizeInt32) -> Size {
    Size {
        width: u32::try_from(size.Width).unwrap_or_default(),
        height: u32::try_from(size.Height).unwrap_or_default(),
    }
}

/// Copies a mapped texture's rows of a `size` picture into `buffer`.
fn copy_rows(
    mapped: &D3D11_MAPPED_SUBRESOURCE,
    size: Size,
    buffer: &mut FrameBuffer,
) -> io::Result<()> {
    let invalid = |error| io::Error::new(io::ErrorKind::InvalidData, error);
    let pitch = usize::try_from(mapped.RowPitch).map_err(io::Error::other)?;
    let length = FrameBuffer::pitched_len(size, pitch).map_err(invalid)?;
    if mapped.pData.is_null() {
        return Err(io::Error::other("the texture was mapped nowhere"));
    }
    #[expect(unsafe_code, reason = "reads a mapped Direct3D texture")]
    // SAFETY: Direct3D mapped `size.height` rows `pitch` bytes apart at
    // `pData`, which is not null, each at least a row of `size.width` pixels
    // long, so the last row ends `length` bytes in, as `pitched_len` counts;
    // the mapping stays valid until `Unmap`, after this slice is gone.
    let source = unsafe { slice::from_raw_parts(mapped.pData.cast::<u8>().cast_const(), length) };
    buffer.fill_pitched(size, source, pitch).map_err(invalid)
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
    use crate::screen::{Image, Layout, NextFrame, Region};
    use crate::system::{Bounds, System};
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
                    && window
                        .bounds
                        .size()
                        .is_some_and(|size| size.width >= 64 && size.height >= 64)
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
                    NextFrame::After(Duration::from_millis(10))
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
                    NextFrame::After(Duration::from_secs(60))
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
                    && window
                        .bounds
                        .size()
                        .is_some_and(|size| size.width >= 256 && size.height >= 256)
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
        let content = system.content_bounds(window.handle).unwrap();
        assert_eq!(Some(size), content.and_then(Bounds::size));
        assert_eq!(image.size(), size);
    }

    #[test]
    fn refuses_a_window_that_does_not_exist() {
        let system = WindowsSystem::new();
        let started = system.start(
            0,
            Crop::Whole,
            Duration::ZERO,
            Box::new(|_| NextFrame::After(Duration::ZERO)),
        );
        assert!(started.is_err());
    }
}
