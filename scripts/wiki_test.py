"""Tests for scripts/wiki.py, which `mise run md:wiki:check` runs."""

import tempfile
import unittest
from pathlib import Path

import wiki

REPO = "owner/name"


class WikiBuildTest(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name) / "repo"
        self.output = Path(self.directory.name) / "wiki"
        self.write("AGENTS.md", "# Agent guide\n")
        self.write("crates/runtime/src/lib.rs", "")
        self.write(
            "docs/user/README.md",
            "# Example user guide\n\n## Get started\n\n[Install it](install.md).\n\n"
            "## Pages\n\n- [Overview](overview.md)\n- [Install, update, and uninstall](install.md#install)\n",
        )
        self.write("docs/user/install.md", "# Install, update, and uninstall\n\n## Install\n\nRun it.\n")
        self.write("docs/user/overview.md", "# Overview\n\nSee ![the window](images/window.png).\n")
        self.write("docs/user/troubleshooting.md", "# Troubleshooting\n\nAsk.\n")
        self.write("docs/user/images/window.png", "picture")
        self.write(
            "docs/developer/README.md",
            "# Developer docs\n\nThe [user guide](../user/README.md) and\n"
            "[`AGENTS.md`](../../AGENTS.md#platform).\n\n## Pages\n\n- [Setup app](setup-app.md)\n",
        )
        self.write("docs/developer/setup-app.md", "# Setup app\n\nSee [install](../user/install.md#install).\n")

    def tearDown(self) -> None:
        self.directory.cleanup()

    def write(self, path: str, text: str) -> None:
        file = self.root / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(text, encoding="utf-8")

    def build(self) -> None:
        wiki.build(self.root, self.output, REPO, "main")

    def page(self, name: str) -> str:
        return (self.output / f"{name}.md").read_text(encoding="utf-8")

    def test_pages_are_named_by_their_titles_and_the_user_guide_is_home(self) -> None:
        self.build()
        self.assertEqual(
            sorted(path.name for path in self.output.glob("*.md")),
            [
                "Developer-docs.md",
                "Home.md",
                "Install,-update,-and-uninstall.md",
                "Overview.md",
                "Setup-app.md",
                "Troubleshooting.md",
                "_Sidebar.md",
            ],
        )

    def test_links_between_pages_name_wiki_pages_and_keep_anchors(self) -> None:
        self.build()
        self.assertIn("[Install it](Install,-update,-and-uninstall).", self.page("Home"))
        self.assertIn("[install](Install,-update,-and-uninstall#install)", self.page("Setup-app"))
        self.assertIn("The [user guide](Home) and", self.page("Developer-docs"))

    def test_links_to_other_repository_files_open_them_on_github(self) -> None:
        self.write("docs/user/troubleshooting.md", "# Troubleshooting\n\nRead [the crate](/crates/runtime).\n")
        self.build()
        self.assertIn(
            "[`AGENTS.md`](https://github.com/owner/name/blob/main/AGENTS.md#platform)", self.page("Developer-docs")
        )
        self.assertIn(
            "[the crate](https://github.com/owner/name/tree/main/crates/runtime)", self.page("Troubleshooting")
        )

    def test_images_are_copied_and_their_links_still_resolve(self) -> None:
        self.build()
        self.assertIn("![the window](user/images/window.png)", self.page("Overview"))
        self.assertEqual((self.output / "user/images/window.png").read_text(encoding="utf-8"), "picture")

    def test_links_inside_code_stay_as_written(self) -> None:
        self.write(
            "docs/user/troubleshooting.md",
            "# Troubleshooting\n\n`[a](install.md)` and [b](install.md)\n\n```md\n[c](install.md)\n```\n",
        )
        self.build()
        page = self.page("Troubleshooting")
        self.assertIn("`[a](install.md)` and [b](Install,-update,-and-uninstall)", page)
        self.assertIn("```md\n[c](install.md)\n```", page)

    def test_link_definitions_are_rewritten(self) -> None:
        self.write(
            "docs/user/troubleshooting.md", "# Troubleshooting\n\nSee [the guide][guide].\n\n[guide]: README.md\n"
        )
        self.build()
        self.assertIn("[guide]: Home\n", self.page("Troubleshooting"))

    def test_a_heading_the_wiki_title_shows_is_dropped(self) -> None:
        self.build()
        self.assertTrue(self.page("Overview").startswith("<!-- Generated from docs/user/overview.md."))
        self.assertNotIn("# Overview", self.page("Overview"))
        self.assertNotIn("# Install, update", self.page("Install,-update,-and-uninstall"))
        self.assertIn("# Example user guide", self.page("Home"))

    def test_each_page_links_the_file_to_edit(self) -> None:
        self.build()
        self.assertIn(
            "generated from [docs/user/install.md](https://github.com/owner/name/blob/main/docs/user/install.md)",
            self.page("Install,-update,-and-uninstall"),
        )

    def test_the_sidebar_follows_each_pages_list_then_titles(self) -> None:
        self.build()
        self.assertEqual(
            self.page("_Sidebar"),
            "**[Example user guide](Home)**\n\n"
            "- [Overview](Overview)\n"
            "- [Install, update, and uninstall](Install,-update,-and-uninstall)\n"
            "- [Troubleshooting](Troubleshooting)\n"
            "\n"
            "**[Developer docs](Developer-docs)**\n\n"
            "- [Setup app](Setup-app)\n",
        )

    def test_a_link_to_a_missing_file_fails_the_build(self) -> None:
        self.write("docs/user/troubleshooting.md", "# Troubleshooting\n\nSee [gone](gone.md).\n")
        with self.assertRaisesRegex(wiki.BuildError, r"docs/user/troubleshooting.md: the link gone.md names no file"):
            self.build()

    def test_two_pages_with_one_title_fail_the_build(self) -> None:
        self.write("docs/developer/overview.md", "# Overview\n")
        with self.assertRaisesRegex(wiki.BuildError, "both make the wiki page Overview"):
            self.build()

    def test_a_page_without_a_title_fails_the_build(self) -> None:
        self.write("docs/user/troubleshooting.md", "Ask.\n")
        with self.assertRaisesRegex(wiki.BuildError, "no level 1 heading"):
            self.build()

    def test_a_page_outside_the_sections_fails_the_build(self) -> None:
        self.write("docs/notes.md", "# Notes\n")
        with self.assertRaisesRegex(wiki.BuildError, "belongs in one of docs/user, docs/developer"):
            self.build()


if __name__ == "__main__":
    unittest.main()
