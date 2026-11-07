# appy-tappy

AppyTappy is a Windows background app that runs automations when another
application reaches a state you can see: a certain number appears in a region
of its window, an icon becomes visible, a bar fills past a threshold, or a
color changes. An automation then acts, such as pausing media or changing an
app's volume.

## Development

Install [mise](https://mise.jdx.dev/), then run:

```sh
mise install
mise run standards:check
```

[`AGENTS.md`](AGENTS.md) describes the workflow and rules, and
[`docs/`](docs/) holds the design pages.

## License

Copyright (C) 2026 Trae Stevens.

AppyTappy is licensed under the GNU General Public License v3.0 only (SPDX: `GPL-3.0-only`). See [LICENSE](LICENSE).
