//! The investigation tools.
//!
//! Tool names are `area_verb`, so later tools slot in beside these:
//! `definition_*` and `rule_*` for app definitions and rules, and
//! `source_test` to try a source.
//!
//! No tool opens an application's process: they list processes and windows
//! from system-wide snapshots and read screen pixels.

mod args;
mod definitions;
mod format;

use appytappy_config::anti_cheat::known_anti_cheat;
use appytappy_sources::system::System;
use serde_json::{Value, json};

use self::args::{ProcessListArgs, ScreenSampleArgs, WindowListArgs, parse};
use self::format::{hex, os_error};

pub(crate) use self::definitions::definitions;

/// The most points one `screen_sample` call reads.
const SCREEN_POINTS_LIMIT: usize = 64;

/// What a tool returns: a JSON result, or an error message for the agent.
pub(crate) type Outcome<T = Value> = Result<T, String>;

/// The tools, over the system they read.
#[derive(Debug)]
pub(crate) struct Session<S: System> {
    system: S,
}

impl<S: System> Session<S> {
    pub(crate) fn new(system: S) -> Self {
        Self { system }
    }

    /// Runs the named tool, or returns `None` for an unknown name.
    pub(crate) fn call(&self, name: &str, arguments: Value) -> Option<Outcome> {
        Some(match name {
            "process_list" => parse(arguments).and_then(|args| self.process_list(args)),
            "window_list" => parse(arguments).and_then(|args| self.window_list(args)),
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

#[cfg(test)]
mod tests {
    use std::io;

    use appytappy_config::anti_cheat::ProcessId;
    use appytappy_sources::system::{Bounds, ProcessEntry, Rgb, WindowEntry, WindowPlace};

    use super::*;

    struct FakeSystem {
        processes: Vec<ProcessEntry>,
    }

    impl System for FakeSystem {
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
            _process: ProcessId,
        ) -> io::Result<appytappy_config::app_file::BuildDef> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
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

    fn session() -> Session<FakeSystem> {
        Session::new(FakeSystem {
            processes: vec![
                entry(10, "Example.exe"),
                entry(20, "BEService.exe"),
                entry(21, "BEService_x64.exe"),
                entry(30, "vgc.exe"),
                entry(40, "browser.exe"),
            ],
        })
    }

    fn call(session: &Session<FakeSystem>, name: &str, arguments: Value) -> Outcome {
        session.call(name, arguments).expect("a known tool")
    }

    #[test]
    fn lists_processes_by_name_and_marks_anti_cheat() {
        let session = session();
        let listed = call(
            &session,
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
        let all = call(&session, "process_list", json!({})).unwrap();
        assert_eq!(all["processes"][0]["executable"], "BEService.exe");
        assert_eq!(all["processes"][2]["anti_cheat"], Value::Null);
        assert_eq!(all["processes"][2]["parent_pid"], 1);
        assert_eq!(all["processes"][2]["threads"], 4);
    }

    #[test]
    fn lists_visible_windows_unless_asked_for_hidden_ones() {
        let session = session();
        let handles = |listed: Value| -> Vec<Value> {
            listed["windows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|window| window["handle"].clone())
                .collect()
        };
        assert_eq!(
            handles(call(&session, "window_list", json!({})).unwrap()),
            [json!("0x1"), json!("0x3")]
        );
        assert_eq!(
            handles(
                call(
                    &session,
                    "window_list",
                    json!({ "pid": 10, "include_hidden": true })
                )
                .unwrap()
            ),
            [json!("0x1"), json!("0x2")]
        );
        let first = &call(&session, "window_list", json!({})).unwrap()["windows"][0];
        assert_eq!(
            first["bounds"],
            json!({ "left": 1, "top": 2, "right": 3, "bottom": 4 })
        );
        assert_eq!(first["pid"], 10);
    }

    #[test]
    fn samples_screen_pixels_within_the_point_limit() {
        let session = session();
        let sampled = call(
            &session,
            "screen_sample",
            json!({ "points": [{ "x": 1, "y": 2 }] }),
        );
        assert_eq!(
            sampled.unwrap(),
            json!({ "pixels": [{ "x": 1, "y": 2, "rgb": "#0102ff" }] })
        );
        let most: Vec<Value> = (0..64).map(|_| json!({ "x": 0, "y": 0 })).collect();
        let sampled = call(&session, "screen_sample", json!({ "points": most }));
        assert_eq!(sampled.unwrap()["pixels"].as_array().unwrap().len(), 64);
        let too_many: Vec<Value> = (0..65).map(|_| json!({ "x": 0, "y": 0 })).collect();
        for points in [Vec::new(), too_many] {
            assert_eq!(
                call(&session, "screen_sample", json!({ "points": points })),
                Err("give from 1 to 64 points".to_owned())
            );
        }
    }

    #[test]
    fn rejects_unknown_tools_and_arguments() {
        let session = session();
        assert_eq!(session.call("memory_read", json!({})), None);
        assert!(
            call(&session, "window_list", json!({ "force": true }))
                .unwrap_err()
                .starts_with("invalid arguments: unknown field `force`")
        );
    }

    #[test]
    fn every_tool_is_read_only_and_has_a_closed_schema() {
        let definitions = definitions();
        let tools = definitions.as_array().unwrap();
        assert_eq!(tools.len(), 3);
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            assert_eq!(tool["annotations"]["readOnlyHint"], true, "{name}");
            assert_eq!(tool["inputSchema"]["additionalProperties"], false, "{name}");
            let session = session();
            assert!(
                session.call(name, json!({})).is_some(),
                "{name} is dispatched"
            );
        }
    }
}
