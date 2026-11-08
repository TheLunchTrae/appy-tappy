//! The investigation tools and the session state they share.
//!
//! Tool names are `area_verb`, so later tools slot in beside these:
//! `definition_*` and `rule_*` for app definitions and rules, and
//! `source_test` to try a source.
//!
//! The anti-cheat block lives here, not in the agent's instructions. Every
//! process starts as class `unknown`. `process_attach` opens one only after
//! the user allows it in a prompt the system shows outside the agent's reach,
//! and never opens a known anti-cheat's own process.

mod args;
mod definitions;
mod format;

use appytappy_config::anti_cheat::{
    AntiCheatClass, ProcessAccessPermit, ProcessId, known_anti_cheat,
};
use appytappy_sources::memory::{
    FirstScan, Module, NextScan, Number, ProcessMemory, Scan, ValueType, follow_pointer_chain,
};
use appytappy_sources::system::{AccessAnswer, ProcessEntry, System};
use serde_json::{Value, json};

use self::args::{
    AttachArgs, FirstMode, FollowPointerArgs, NextMode, NoArgs, OffsetArg, ProcessListArgs,
    ReadArgs, ReadBytesArgs, ScanFirstArgs, ScanNextArgs, ScanResultsArgs, ScreenSampleArgs,
    WindowListArgs, exact, parse, range, value_type,
};
use self::format::{
    hex, module_offset, no_scan, not_attached, number_json, os_error, scan_summary,
};

pub(crate) use self::definitions::definitions;

/// The most results one `memory_scan_results` call returns.
const RESULTS_LIMIT: usize = 200;

/// The most bytes one `memory_read_bytes` call returns.
const READ_BYTES_LIMIT: usize = 1024;

/// The most points one `screen_sample` call reads.
const SCREEN_POINTS_LIMIT: usize = 64;

/// What a tool returns: a JSON result, or an error message for the agent.
pub(crate) type Outcome<T = Value> = Result<T, String>;

/// The state the tools share: the programs the user refused, and the one
/// attached process with its scan.
#[derive(Debug)]
pub(crate) struct Session<S: System> {
    system: S,
    /// Executables the user refused this session, lowercased. The server
    /// refuses them again without asking, so an agent cannot wear the user
    /// down with repeated prompts.
    refused: Vec<String>,
    attached: Option<Attached<S::Process>>,
}

#[derive(Debug)]
struct Attached<P> {
    entry: ProcessEntry,
    process: P,
    modules: Vec<Module>,
    scan: Option<Scan>,
}

impl<S: System> Session<S> {
    pub(crate) fn new(system: S) -> Self {
        Self {
            system,
            refused: Vec::new(),
            attached: None,
        }
    }

    /// Runs the named tool, or returns `None` for an unknown name.
    pub(crate) fn call(&mut self, name: &str, arguments: Value) -> Option<Outcome> {
        Some(match name {
            "process_list" => parse(arguments).and_then(|args| self.process_list(args)),
            "window_list" => parse(arguments).and_then(|args| self.window_list(args)),
            "process_attach" => parse(arguments).and_then(|args| self.process_attach(args)),
            "process_detach" => parse(arguments).and_then(|NoArgs {}| self.process_detach()),
            "memory_scan_first" => parse(arguments).and_then(|args| self.scan_first(args)),
            "memory_scan_next" => parse(arguments).and_then(|args| self.scan_next(args)),
            "memory_scan_results" => parse(arguments).and_then(|args| self.scan_results(args)),
            "memory_read" => parse(arguments).and_then(|args| self.read(args)),
            "memory_read_bytes" => parse(arguments).and_then(|args| self.read_bytes(args)),
            "memory_follow_pointer" => parse(arguments).and_then(|args| self.follow_pointer(args)),
            "screen_sample" => parse(arguments).and_then(|args| self.screen_sample(args)),
            _ => return None,
        })
    }

    fn process_list(&self, args: ProcessListArgs) -> Outcome {
        let mut processes = self
            .system
            .processes()
            .map_err(os_error("list processes"))?;
        if let Some(needle) = &args.name_contains {
            let needle = needle.to_lowercase();
            processes.retain(|entry| entry.executable.to_lowercase().contains(&needle));
        }
        processes.sort_by_key(|entry| (entry.executable.to_lowercase(), entry.id));
        let processes: Vec<Value> = processes
            .iter()
            .map(|entry| {
                json!({
                    "pid": entry.id.raw(),
                    "parent_pid": entry.parent.raw(),
                    "executable": entry.executable,
                    "threads": entry.threads,
                    "anti_cheat": known_anti_cheat(&entry.executable).map(|known| known.product),
                })
            })
            .collect();
        Ok(json!({ "processes": processes }))
    }

    fn window_list(&self, args: WindowListArgs) -> Outcome {
        let windows = self.system.windows().map_err(os_error("list windows"))?;
        let windows: Vec<Value> = windows
            .iter()
            .filter(|window| args.include_hidden || window.visible)
            .filter(|window| args.pid.is_none_or(|pid| window.process.raw() == pid))
            .map(|window| {
                json!({
                    "handle": hex(window.handle),
                    "pid": window.process.raw(),
                    "title": window.title,
                    "class": window.class,
                    "visible": window.visible,
                    "bounds": {
                        "left": window.bounds.left,
                        "top": window.bounds.top,
                        "right": window.bounds.right,
                        "bottom": window.bounds.bottom,
                    },
                })
            })
            .collect();
        Ok(json!({ "windows": windows }))
    }

    /// Attaches to the process `args` names. The process attached before
    /// stays attached until the new one is ready, so a failed attach keeps
    /// it and its scan. The error names it, unless the user refused its
    /// program, because the error then says the program is refused.
    fn process_attach(&mut self, args: AttachArgs) -> Outcome {
        match self.open_attachment(args) {
            Ok((attached, result)) => {
                self.attached = Some(attached);
                Ok(result)
            },
            Err(error) => {
                let still = self
                    .attached
                    .as_ref()
                    .map(|old| &old.entry)
                    .filter(|old| !self.refused.contains(&old.executable.to_lowercase()));
                Err(attach_error(error, still))
            },
        }
    }

    /// Opens and checks the process `args` names, and describes it.
    fn open_attachment(
        &mut self,
        args: AttachArgs,
    ) -> Result<(Attached<S::Process>, Value), String> {
        let processes = self
            .system
            .processes()
            .map_err(os_error("list processes"))?;
        let id = ProcessId::new(args.pid);
        let entry = processes
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or_else(|| format!("no process has ID {id}"))?;
        let class = self.classify(&entry, &processes)?;
        let permit = ProcessAccessPermit::issue(id, class).map_err(|blocked| {
            if blocked.class() == AntiCheatClass::Unknown {
                format!(
                    "the user did not allow reading {}'s memory, and AppyTappy refuses it for \
                     the rest of this session; investigate with window_list and screen_sample instead",
                    entry.executable
                )
            } else {
                format!("{blocked}: {} is an anti-cheat process", entry.executable)
            }
        })?;
        let process = self
            .system
            .open(&permit)
            .map_err(os_error("open the process"))?;
        // The process could have exited while the user decided, and its ID gone
        // to another program. The open handle pins the ID, so check that it still
        // runs the program the user allowed.
        let image = process
            .image_name()
            .map_err(os_error("read the process's program name"))?;
        if !image.eq_ignore_ascii_case(&entry.executable) || known_anti_cheat(&image).is_some() {
            return Err(format!(
                "process {id} now runs {image}, not the {} the user allowed; it was closed",
                entry.executable
            ));
        }
        let modules = process.modules().map_err(os_error("list modules"))?;
        let scannable = process
            .regions()
            .map_err(os_error("list memory regions"))?
            .iter()
            .fold(0_u64, |total, region| total.saturating_add(region.size));
        let result = json!({
            "pid": id.raw(),
            "executable": entry.executable,
            "pointer_bits": process.pointer_width().bits(),
            "scannable_bytes": scannable,
            "modules": modules.iter().map(|module| json!({
                "name": module.name,
                "base": hex(module.base),
                "size": module.size,
            })).collect::<Vec<_>>(),
        });
        let attached = Attached {
            entry,
            process,
            modules,
            scan: None,
        };
        Ok((attached, result))
    }

    /// The anti-cheat class to apply to `entry`: a known anti-cheat's own
    /// class, `none` when the user allows it now, and `unknown` otherwise.
    /// Each attach asks again, so an approval never carries over to a later
    /// process that reuses the ID.
    fn classify(
        &mut self,
        entry: &ProcessEntry,
        processes: &[ProcessEntry],
    ) -> Result<AntiCheatClass, String> {
        if let Some(known) = known_anti_cheat(&entry.executable) {
            return Ok(known.class);
        }
        let executable = entry.executable.to_lowercase();
        if self.refused.contains(&executable) {
            return Ok(AntiCheatClass::Unknown);
        }
        let mut running: Vec<&str> = processes
            .iter()
            .filter_map(|process| known_anti_cheat(&process.executable))
            .map(|known| known.product)
            .collect();
        running.sort_unstable();
        running.dedup();
        match self
            .system
            .ask_memory_access(entry, &running)
            .map_err(os_error("ask the user"))?
        {
            AccessAnswer::Allowed => Ok(AntiCheatClass::None),
            AccessAnswer::Refused => {
                self.refused.push(executable);
                Ok(AntiCheatClass::Unknown)
            },
        }
    }

    fn process_detach(&mut self) -> Outcome {
        let attached = self.attached.take().ok_or_else(not_attached)?;
        Ok(json!({ "detached": attached.entry.executable }))
    }

    fn attached(&mut self) -> Result<&mut Attached<S::Process>, String> {
        self.attached.as_mut().ok_or_else(not_attached)
    }

    fn scan_first(&mut self, args: ScanFirstArgs) -> Outcome {
        let value_type = value_type(&args.value_type)?;
        let scan = match args.mode {
            FirstMode::Unknown => FirstScan::Unknown,
            FirstMode::Exact => FirstScan::Value(exact(args.value)?),
            FirstMode::Range => FirstScan::Value(range(args.min, args.max)?),
        };
        let attached = self.attached()?;
        // A snapshot can be as large as the previous one, so the previous
        // scan is dropped first rather than held while the new one is made.
        let dropped = attached.scan.take().is_some();
        let scan = Scan::first(&attached.process, value_type, scan).map_err(|error| {
            if dropped {
                format!("{error}; the previous scan was dropped")
            } else {
                error.to_string()
            }
        })?;
        let summary = scan_summary(&scan);
        attached.scan = Some(scan);
        Ok(summary)
    }

    fn scan_next(&mut self, args: ScanNextArgs) -> Outcome {
        let scan = match args.mode {
            NextMode::Exact => NextScan::Value(exact(args.value)?),
            NextMode::Range => NextScan::Value(range(args.min, args.max)?),
            NextMode::Changed => NextScan::Changed,
            NextMode::Unchanged => NextScan::Unchanged,
            NextMode::Increased => NextScan::Increased,
            NextMode::Decreased => NextScan::Decreased,
        };
        let attached = self.attached()?;
        let current = attached.scan.as_mut().ok_or_else(no_scan)?;
        current
            .next(&attached.process, scan)
            .map_err(|error| error.to_string())?;
        Ok(scan_summary(current))
    }

    fn scan_results(&mut self, args: ScanResultsArgs) -> Outcome {
        let limit = args.limit.unwrap_or(20).min(RESULTS_LIMIT);
        let attached = self.attached()?;
        let scan = attached.scan.as_ref().ok_or_else(no_scan)?;
        let value_type = scan.value_type();
        let candidates = scan.candidates().ok_or(
            "the unknown-value scan has not been narrowed yet; run memory_scan_next first",
        )?;
        let results: Vec<Value> = candidates
            .iter()
            .skip(args.offset)
            .take(limit)
            .map(|candidate| {
                json!({
                    "address": hex(candidate.address),
                    "module_offset": module_offset(&attached.modules, candidate.address),
                    "previous": candidate.previous(value_type).map(number_json),
                    "current": read_value(&attached.process, candidate.address, value_type).ok().map(number_json),
                })
            })
            .collect();
        Ok(json!({ "total": candidates.len(), "results": results }))
    }

    fn read(&mut self, args: ReadArgs) -> Outcome {
        let address = args.address.parse()?;
        let value_type = value_type(&args.value_type)?;
        let attached = self.attached()?;
        let value = read_value(&attached.process, address, value_type)?;
        Ok(json!({ "address": hex(address), "value": number_json(value) }))
    }

    fn read_bytes(&mut self, args: ReadBytesArgs) -> Outcome {
        let address = args.address.parse()?;
        if args.length == 0 || args.length > READ_BYTES_LIMIT {
            return Err(format!("length must be from 1 to {READ_BYTES_LIMIT}"));
        }
        let attached = self.attached()?;
        let mut bytes = vec![0; args.length];
        attached
            .process
            .read(address, &mut bytes)
            .map_err(os_error("read memory"))?;
        let hex_bytes: Vec<String> = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(json!({ "address": hex(address), "bytes": hex_bytes.join(" ") }))
    }

    fn follow_pointer(&mut self, args: FollowPointerArgs) -> Outcome {
        let base = args.base.parse()?;
        let offsets = args
            .offsets
            .iter()
            .map(OffsetArg::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let value_type = args.value_type.as_deref().map(value_type).transpose()?;
        let attached = self.attached()?;
        let start = match &args.module {
            None => base,
            Some(name) => attached
                .modules
                .iter()
                .find(|module| module.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("no loaded module is named {name}"))?
                .base
                .checked_add(base)
                .ok_or("the module base plus the offset overflows")?,
        };
        let steps = follow_pointer_chain(&attached.process, start, &offsets)
            .map_err(|error| error.to_string())?;
        let address = steps.last().map_or(start, |step| step.address);
        let value = value_type
            .map(|kind| read_value(&attached.process, address, kind))
            .transpose()?
            .map(number_json);
        Ok(json!({
            "start": hex(start),
            "steps": steps.iter().map(|step| json!({
                "read_at": hex(step.read_at),
                "pointer": hex(step.pointer),
                "address": hex(step.address),
            })).collect::<Vec<_>>(),
            "address": hex(address),
            "value": value,
        }))
    }

    fn screen_sample(&self, args: ScreenSampleArgs) -> Outcome {
        if args.points.is_empty() || args.points.len() > SCREEN_POINTS_LIMIT {
            return Err(format!("give from 1 to {SCREEN_POINTS_LIMIT} points"));
        }
        let pixels = args
            .points
            .iter()
            .map(|point| {
                let pixel = self
                    .system
                    .screen_pixel(point.x, point.y)
                    .map_err(os_error("read the screen"))?;
                Ok(json!({
                    "x": point.x,
                    "y": point.y,
                    "rgb": format!("#{:02x}{:02x}{:02x}", pixel.red, pixel.green, pixel.blue),
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(json!({ "pixels": pixels }))
    }
}

fn read_value(memory: &impl ProcessMemory, address: u64, value_type: ValueType) -> Outcome<Number> {
    let mut bytes = vec![0; value_type.size()];
    memory
        .read(address, &mut bytes)
        .map_err(os_error("read memory"))?;
    value_type
        .decode(&bytes)
        .ok_or_else(|| "the read returned too few bytes".to_owned())
}

/// The error of a failed attach, naming the process `still` that stays
/// attached, if any.
fn attach_error(error: String, still: Option<&ProcessEntry>) -> String {
    match still {
        Some(entry) => format!(
            "{error}; process {} ({}) is still attached",
            entry.id, entry.executable
        ),
        None => error,
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::mpsc;

    use appytappy_sources::memory::{PointerWidth, Region};
    use appytappy_sources::system::{Bounds, Rgb, WindowEntry, WindowPlace};

    use super::*;

    const BASE: u64 = 0x1_0000;

    struct FakeProcess {
        bytes: Vec<u8>,
        image: String,
        /// Whether listing its memory regions fails.
        unlisted: bool,
    }

    impl FakeProcess {
        fn write(&mut self, address: u64, bytes: &[u8]) {
            let offset = usize::try_from(address.checked_sub(BASE).unwrap()).unwrap();
            self.bytes[offset..]
                .iter_mut()
                .zip(bytes)
                .for_each(|(slot, byte)| *slot = *byte);
        }
    }

    impl ProcessMemory for FakeProcess {
        fn regions(&self) -> io::Result<Vec<Region>> {
            if self.unlisted {
                return Err(io::Error::other("no regions"));
            }
            Ok(vec![Region {
                base: BASE,
                size: self.bytes.len() as u64,
            }])
        }

        fn read(&self, address: u64, buffer: &mut [u8]) -> io::Result<()> {
            let source = address
                .checked_sub(BASE)
                .and_then(|offset| usize::try_from(offset).ok())
                .and_then(|offset| self.bytes.get(offset..offset.checked_add(buffer.len())?))
                .ok_or_else(|| io::Error::other("unmapped"))?;
            buffer.copy_from_slice(source);
            Ok(())
        }

        fn modules(&self) -> io::Result<Vec<Module>> {
            Ok(vec![Module {
                name: "Example.exe".to_owned(),
                base: BASE,
                size: 0x1000,
            }])
        }

        fn pointer_width(&self) -> PointerWidth {
            PointerWidth::Bits64
        }

        fn image_name(&self) -> io::Result<String> {
            Ok(self.image.clone())
        }
    }

    struct FakeSystem {
        processes: Vec<ProcessEntry>,
        /// The program an opened process turns out to run.
        image: String,
        answer: AccessAnswer,
        asked: mpsc::Sender<(ProcessId, Vec<String>)>,
    }

    impl System for FakeSystem {
        type Process = FakeProcess;

        fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
            Ok(self.processes.clone())
        }

        fn windows(&self) -> io::Result<Vec<WindowEntry>> {
            let window = |handle, process, visible| WindowEntry {
                handle,
                process: ProcessId::new(process),
                title: format!("window {handle}"),
                class: "Class".to_owned(),
                visible,
                bounds: Bounds {
                    left: 1,
                    top: 2,
                    right: 3,
                    bottom: 4,
                },
            };
            Ok(vec![
                window(1, 10, true),
                window(2, 10, false),
                window(3, 11, true),
            ])
        }

        fn window_place(&self, _window: u64) -> io::Result<Option<WindowPlace>> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }

        fn executable_build(
            &self,
            _process: appytappy_config::anti_cheat::ProcessId,
        ) -> io::Result<appytappy_config::app_file::BuildDef> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }

        fn ask_memory_access(
            &self,
            process: &ProcessEntry,
            anti_cheat: &[&str],
        ) -> io::Result<AccessAnswer> {
            self.asked
                .send((
                    process.id,
                    anti_cheat.iter().map(|name| (*name).to_owned()).collect(),
                ))
                .unwrap();
            Ok(self.answer)
        }

        fn open(&self, permit: &ProcessAccessPermit) -> io::Result<FakeProcess> {
            assert_eq!(permit.process(), ProcessId::new(10));
            Ok(FakeProcess {
                bytes: vec![0; 0x1000],
                image: self.image.clone(),
                unlisted: false,
            })
        }

        fn screen_pixel(&self, x: i32, y: i32) -> io::Result<Rgb> {
            Ok(Rgb {
                red: u8::try_from(x).unwrap(),
                green: u8::try_from(y).unwrap(),
                blue: 0xff,
            })
        }
    }

    fn entry(id: u32, executable: &str) -> ProcessEntry {
        ProcessEntry {
            id: ProcessId::new(id),
            parent: ProcessId::new(1),
            executable: executable.to_owned(),
            threads: 4,
        }
    }

    fn session(
        answer: AccessAnswer,
    ) -> (
        Session<FakeSystem>,
        mpsc::Receiver<(ProcessId, Vec<String>)>,
    ) {
        let (asked, questions) = mpsc::channel();
        let system = FakeSystem {
            processes: vec![
                entry(10, "Example.exe"),
                entry(20, "BEService.exe"),
                entry(21, "BEService_x64.exe"),
                entry(30, "vgc.exe"),
                entry(40, "browser.exe"),
            ],
            image: "EXAMPLE.EXE".to_owned(),
            answer,
            asked,
        };
        (Session::new(system), questions)
    }

    fn call(session: &mut Session<FakeSystem>, name: &str, arguments: Value) -> Outcome {
        session.call(name, arguments).expect("a known tool")
    }

    /// A session attached to the app, with the receiver that keeps the
    /// fake's question channel open.
    fn attached() -> (
        Session<FakeSystem>,
        mpsc::Receiver<(ProcessId, Vec<String>)>,
    ) {
        let (mut session, questions) = session(AccessAnswer::Allowed);
        call(&mut session, "process_attach", json!({ "pid": 10 })).unwrap();
        (session, questions)
    }

    impl Session<FakeSystem> {
        fn process_mut(&mut self) -> &mut FakeProcess {
            &mut self.attached.as_mut().unwrap().process
        }
    }

    #[test]
    fn attach_asks_the_user_once_and_names_running_anti_cheat() {
        let (mut session, questions) = session(AccessAnswer::Allowed);
        let attached = call(&mut session, "process_attach", json!({ "pid": 10 })).unwrap();
        assert_eq!(
            attached,
            json!({
                "pid": 10,
                "executable": "Example.exe",
                "pointer_bits": 64,
                "scannable_bytes": 0x1000,
                "modules": [{ "name": "Example.exe", "base": "0x10000", "size": 0x1000 }],
            })
        );
        assert_eq!(
            questions.try_recv().unwrap(),
            (
                ProcessId::new(10),
                vec!["BattlEye".to_owned(), "Riot Vanguard".to_owned()]
            )
        );

        call(&mut session, "process_attach", json!({ "pid": 10 })).unwrap();
        assert!(
            questions.try_recv().is_ok(),
            "each attach asks again, so an approval never outlives its process"
        );
    }

    #[test]
    fn attach_closes_a_process_whose_id_now_runs_another_program() {
        let (mut session, _questions) = session(AccessAnswer::Allowed);
        for image in ["other.exe", "Example.exe.bak", "BEService.exe"] {
            image.clone_into(&mut session.system.image);
            assert_eq!(
                call(&mut session, "process_attach", json!({ "pid": 10 })),
                Err(format!(
                    "process 10 now runs {image}, not the Example.exe the user allowed; it was closed"
                ))
            );
            assert!(session.attached.is_none());
        }
    }

    #[test]
    fn a_failed_attach_keeps_the_process_attached_before_and_its_scan() {
        let (mut session, _questions) = attached();
        session
            .process_mut()
            .write(BASE + 0x10, &1234_i32.to_le_bytes());
        let first = json!({ "value_type": "i32", "mode": "exact", "value": 1234 });
        call(&mut session, "memory_scan_first", first).unwrap();
        "other.exe".clone_into(&mut session.system.image);
        assert_eq!(
            call(&mut session, "process_attach", json!({ "pid": 10 })),
            Err(
                "process 10 now runs other.exe, not the Example.exe the user allowed; it was \
                 closed; process 10 (Example.exe) is still attached"
                    .to_owned()
            )
        );
        assert_eq!(
            call(&mut session, "memory_scan_results", json!({})).unwrap()["total"],
            1,
            "the scan is still there"
        );
        assert_eq!(
            session.attached.as_ref().unwrap().process.image,
            "EXAMPLE.EXE",
            "the process attached first, not the one that failed its check"
        );
    }

    #[test]
    fn refusing_the_attached_program_does_not_call_it_still_attached() {
        let (mut session, _questions) = attached();
        session.system.answer = AccessAnswer::Refused;
        let error = call(&mut session, "process_attach", json!({ "pid": 10 })).unwrap_err();
        assert!(
            error.starts_with("the user did not allow reading Example.exe's memory"),
            "{error}"
        );
        assert!(!error.contains("still attached"), "{error}");
        assert!(session.attached.is_some(), "the process stays attached");
    }

    #[test]
    fn a_failed_first_scan_says_it_dropped_the_previous_one() {
        let (mut session, _questions) = attached();
        let first = json!({ "value_type": "i32", "mode": "exact", "value": 0 });
        session.process_mut().unlisted = true;
        let error = call(&mut session, "memory_scan_first", first.clone()).unwrap_err();
        assert!(
            !error.contains("dropped"),
            "there was no scan to drop: {error}"
        );
        session.process_mut().unlisted = false;
        call(&mut session, "memory_scan_first", first.clone()).unwrap();
        session.process_mut().unlisted = true;
        let error = call(&mut session, "memory_scan_first", first).unwrap_err();
        assert!(
            error.ends_with("; the previous scan was dropped"),
            "{error}"
        );
        assert_eq!(
            call(&mut session, "memory_scan_results", json!({})),
            Err("no scan is running; call memory_scan_first first".to_owned())
        );
    }

    #[test]
    fn attach_opens_nothing_the_user_refused_or_that_is_anti_cheat() {
        let (mut session, questions) = session(AccessAnswer::Refused);
        let refused = call(&mut session, "process_attach", json!({ "pid": 10 })).unwrap_err();
        assert!(refused.starts_with("the user did not allow reading Example.exe's memory"));
        assert_eq!(
            call(&mut session, "process_attach", json!({ "pid": 10 })),
            Err(refused),
            "a refused program is refused again without asking"
        );
        assert_eq!(
            call(
                &mut session,
                "memory_read",
                json!({ "address": 1, "value_type": "i32" })
            ),
            Err("no process is attached; call process_attach first".to_owned())
        );

        let anti_cheat = call(&mut session, "process_attach", json!({ "pid": 20 })).unwrap_err();
        assert_eq!(
            anti_cheat,
            "process 20 has anti-cheat class kernel-level, so AppyTappy does not open it: BEService.exe is an anti-cheat process"
        );
        assert_eq!(
            questions.try_iter().count(),
            1,
            "only the app was asked about"
        );

        assert_eq!(
            call(&mut session, "process_attach", json!({ "pid": 99 })),
            Err("no process has ID 99".to_owned())
        );
    }

    #[test]
    fn scans_narrow_to_the_changing_value() {
        let (mut session, _questions) = attached();
        session
            .process_mut()
            .write(BASE + 0x10, &10.0_f32.to_le_bytes());
        let first = json!({ "value_type": "f32", "mode": "unknown" });
        assert_eq!(
            call(&mut session, "memory_scan_first", first).unwrap(),
            json!({ "value_type": "f32", "candidates": 0x400, "narrowed": false })
        );
        assert!(
            call(&mut session, "memory_scan_results", json!({}))
                .unwrap_err()
                .contains("not been narrowed")
        );

        session
            .process_mut()
            .write(BASE + 0x10, &20.0_f32.to_le_bytes());
        assert_eq!(
            call(
                &mut session,
                "memory_scan_next",
                json!({ "mode": "increased" })
            )
            .unwrap(),
            json!({ "value_type": "f32", "candidates": 1, "narrowed": true })
        );
        session
            .process_mut()
            .write(BASE + 0x10, &25.0_f32.to_le_bytes());
        assert_eq!(
            call(&mut session, "memory_scan_results", json!({})).unwrap(),
            json!({
                "total": 1,
                "results": [{
                    "address": "0x10010",
                    "module_offset": "Example.exe+0x10",
                    "previous": 20.0,
                    "current": 25.0,
                }],
            })
        );
        let exact = json!({ "mode": "exact", "value": 25 });
        assert_eq!(
            call(&mut session, "memory_scan_next", exact).unwrap()["candidates"],
            1
        );
        let range = json!({ "mode": "range", "min": 26, "max": 30 });
        assert_eq!(
            call(&mut session, "memory_scan_next", range).unwrap()["candidates"],
            0
        );
    }

    #[test]
    fn scan_results_page_through_candidates() {
        let (mut session, _questions) = attached();
        for index in 0_u64..5 {
            session
                .process_mut()
                .write(BASE + index * 8, &7_i32.to_le_bytes());
        }
        let first = json!({ "value_type": "i32", "mode": "exact", "value": 7 });
        assert_eq!(
            call(&mut session, "memory_scan_first", first).unwrap()["candidates"],
            5
        );
        let page = call(
            &mut session,
            "memory_scan_results",
            json!({ "offset": 1, "limit": 2 }),
        )
        .unwrap();
        assert_eq!(page["total"], 5);
        let addresses: Vec<&Value> = page["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|result| &result["address"])
            .collect();
        assert_eq!(addresses, [&json!("0x10008"), &json!("0x10010")]);
        assert_eq!(page["results"][0]["module_offset"], "Example.exe+0x8");
        let capped = call(&mut session, "memory_scan_results", json!({ "limit": 500 })).unwrap();
        assert_eq!(capped["results"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn scan_modes_require_their_values() {
        let (mut session, _questions) = attached();
        assert_eq!(
            call(
                &mut session,
                "memory_scan_next",
                json!({ "mode": "changed" })
            ),
            Err("no scan is running; call memory_scan_first first".to_owned())
        );
        let missing = [
            (
                json!({ "value_type": "i32", "mode": "exact" }),
                "mode exact needs value",
            ),
            (
                json!({ "value_type": "i32", "mode": "range", "min": 1 }),
                "mode range needs min and max",
            ),
            (
                json!({ "value_type": "i16", "mode": "unknown" }),
                "unknown value type: i16",
            ),
        ];
        for (arguments, error) in missing {
            assert_eq!(
                call(&mut session, "memory_scan_first", arguments),
                Err(error.to_owned())
            );
        }
        assert!(
            call(
                &mut session,
                "memory_scan_first",
                json!({ "mode": "unknown" })
            )
            .unwrap_err()
            .starts_with("invalid arguments: missing field `value_type`")
        );
    }

    #[test]
    fn reads_values_bytes_and_pointer_chains() {
        let (mut session, _questions) = attached();
        session
            .process_mut()
            .write(BASE + 0x20, &(BASE + 0x30).to_le_bytes());
        session
            .process_mut()
            .write(BASE + 0x38, &77_i32.to_le_bytes());

        let chain = json!({ "module": "example.exe", "base": "0x20", "offsets": ["0x8"], "value_type": "i32" });
        assert_eq!(
            call(&mut session, "memory_follow_pointer", chain).unwrap(),
            json!({
                "start": "0x10020",
                "steps": [{ "read_at": "0x10020", "pointer": "0x10030", "address": "0x10038" }],
                "address": "0x10038",
                "value": 77,
            })
        );
        let absolute = json!({ "base": 0x1_0038, "offsets": [] });
        assert_eq!(
            call(&mut session, "memory_follow_pointer", absolute).unwrap()["value"],
            Value::Null
        );
        let negative = json!({ "base": "65568", "offsets": ["-0x8"], "value_type": "u64" });
        assert_eq!(
            call(&mut session, "memory_follow_pointer", negative).unwrap()["address"],
            "0x10028"
        );
        assert_eq!(
            call(
                &mut session,
                "memory_follow_pointer",
                json!({ "module": "x.dll", "base": 0 })
            ),
            Err("no loaded module is named x.dll".to_owned())
        );

        let read = json!({ "address": "0x10038", "value_type": "i32" });
        assert_eq!(
            call(&mut session, "memory_read", read).unwrap(),
            json!({ "address": "0x10038", "value": 77 })
        );
        let bytes = json!({ "address": "0x10038", "length": 4 });
        assert_eq!(
            call(&mut session, "memory_read_bytes", bytes).unwrap(),
            json!({ "address": "0x10038", "bytes": "4d 00 00 00" })
        );
        let longest = call(
            &mut session,
            "memory_read_bytes",
            json!({ "address": "0x10000", "length": 1024 }),
        );
        assert_eq!(
            longest.unwrap()["bytes"].as_str().unwrap().len(),
            1024 * 3 - 1
        );
        for length in [0, 1025] {
            assert_eq!(
                call(
                    &mut session,
                    "memory_read_bytes",
                    json!({ "address": 0, "length": length })
                ),
                Err("length must be from 1 to 1024".to_owned())
            );
        }
        assert!(
            call(
                &mut session,
                "memory_read",
                json!({ "address": "0xzz", "value_type": "i32" })
            )
            .unwrap_err()
            .contains("not an address: 0xzz")
        );
    }

    #[test]
    fn detach_drops_the_process() {
        let (mut session, _questions) = attached();
        assert_eq!(
            call(&mut session, "process_detach", json!({})).unwrap(),
            json!({ "detached": "Example.exe" })
        );
        assert_eq!(
            call(&mut session, "process_detach", json!({})),
            Err("no process is attached; call process_attach first".to_owned())
        );
    }

    #[test]
    fn lists_processes_by_name_and_marks_anti_cheat() {
        let (mut session, _) = session(AccessAnswer::Refused);
        let listed = call(
            &mut session,
            "process_list",
            json!({ "name_contains": "SERVICE" }),
        )
        .unwrap();
        let names: Vec<(&Value, &Value)> = listed["processes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|process| (&process["executable"], &process["anti_cheat"]))
            .collect();
        assert_eq!(
            names,
            [
                (&json!("BEService.exe"), &json!("BattlEye")),
                (&json!("BEService_x64.exe"), &json!("BattlEye")),
            ]
        );
        let all = call(&mut session, "process_list", json!({})).unwrap();
        assert_eq!(all["processes"][0]["executable"], "BEService.exe");
        assert_eq!(all["processes"][2]["anti_cheat"], Value::Null);
        assert_eq!(all["processes"][2]["parent_pid"], 1);
        assert_eq!(all["processes"][2]["threads"], 4);
    }

    #[test]
    fn lists_visible_windows_unless_asked_for_hidden_ones() {
        let (mut session, _) = session(AccessAnswer::Refused);
        let handles = |listed: Value| -> Vec<Value> {
            listed["windows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|window| window["handle"].clone())
                .collect()
        };
        assert_eq!(
            handles(call(&mut session, "window_list", json!({})).unwrap()),
            [json!("0x1"), json!("0x3")]
        );
        assert_eq!(
            handles(
                call(
                    &mut session,
                    "window_list",
                    json!({ "pid": 10, "include_hidden": true })
                )
                .unwrap()
            ),
            [json!("0x1"), json!("0x2")]
        );
        let first = &call(&mut session, "window_list", json!({})).unwrap()["windows"][0];
        assert_eq!(
            first["bounds"],
            json!({ "left": 1, "top": 2, "right": 3, "bottom": 4 })
        );
        assert_eq!(first["pid"], 10);
    }

    #[test]
    fn samples_screen_pixels_within_the_point_limit() {
        let (mut session, _) = session(AccessAnswer::Refused);
        let sampled = call(
            &mut session,
            "screen_sample",
            json!({ "points": [{ "x": 1, "y": 2 }] }),
        );
        assert_eq!(
            sampled.unwrap(),
            json!({ "pixels": [{ "x": 1, "y": 2, "rgb": "#0102ff" }] })
        );
        let most: Vec<Value> = (0..64).map(|_| json!({ "x": 0, "y": 0 })).collect();
        let sampled = call(&mut session, "screen_sample", json!({ "points": most }));
        assert_eq!(sampled.unwrap()["pixels"].as_array().unwrap().len(), 64);
        let too_many: Vec<Value> = (0..65).map(|_| json!({ "x": 0, "y": 0 })).collect();
        for points in [Vec::new(), too_many] {
            assert_eq!(
                call(&mut session, "screen_sample", json!({ "points": points })),
                Err("give from 1 to 64 points".to_owned())
            );
        }
    }

    #[test]
    fn rejects_unknown_tools_and_arguments() {
        let (mut session, _) = session(AccessAnswer::Refused);
        assert_eq!(session.call("memory_write", json!({})), None);
        assert!(
            call(&mut session, "process_detach", json!({ "force": true }))
                .unwrap_err()
                .starts_with("invalid arguments: unknown field `force`")
        );
    }

    #[test]
    fn every_tool_is_read_only_and_has_a_closed_schema() {
        let definitions = definitions();
        let tools = definitions.as_array().unwrap();
        assert_eq!(tools.len(), 11);
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            assert_eq!(tool["annotations"]["readOnlyHint"], true, "{name}");
            assert_eq!(tool["inputSchema"]["additionalProperties"], false, "{name}");
            let (mut session, _) = session(AccessAnswer::Refused);
            assert!(
                session.call(name, json!({})).is_some(),
                "{name} is dispatched"
            );
        }
    }
}
