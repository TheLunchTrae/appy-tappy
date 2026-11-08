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
        self.write("wiki/Home.md", "# Example user guide\n\n## Get started\n\n[Install it](user/Install.md).\n")
        self.write(
            "wiki/_Sidebar.md",
            "**[Guide](Home.md)**\n\n- [Install](user/Install.md#install)\n- [Overview](user/Overview.md)\n"
            "- [Troubleshooting](user/Troubleshooting.md)\n- [Developer docs](developer/Developer-docs.md)\n"
            "- [Setup app](developer/Setup-app.md)\n",
        )
        self.write("wiki/user/Install.md", "# Install\n\n## Install\n\nRun it.\n")
        self.write("wiki/user/Overview.md", "# Overview\n\nSee ![the window](images/window.png).\n")
        self.write("wiki/user/Troubleshooting.md", "# Troubleshooting\n\nAsk.\n")
        self.write("wiki/user/images/window.png", "picture")
        self.write(
            "wiki/developer/Developer-docs.md",
            "# Developer docs\n\nThe [user guide](../Home.md) and\n[`AGENTS.md`](../../AGENTS.md#platform).\n",
        )
        self.write("wiki/developer/Setup-app.md", "# Setup app\n\nSee [install](../user/Install.md#install).\n")

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

    def test_pages_are_named_by_their_files_at_the_top_of_the_wiki(self) -> None:
        self.build()
        self.assertEqual(
            sorted(path.name for path in self.output.glob("*.md")),
            [
                "Developer-docs.md",
                "Home.md",
                "Install.md",
                "Overview.md",
                "Setup-app.md",
                "Troubleshooting.md",
                "_Sidebar.md",
            ],
        )

    def test_links_between_pages_name_wiki_pages_and_keep_anchors(self) -> None:
        self.build()
        self.assertIn("[Install it](Install).", self.page("Home"))
        self.assertIn("[install](Install#install)", self.page("Setup-app"))
        self.assertIn("The [user guide](Home) and", self.page("Developer-docs"))

    def test_page_names_with_commas_link_unescaped(self) -> None:
        self.write("wiki/user/Install,-update.md", "# Install, update\n")
        self.write("wiki/user/Troubleshooting.md", "# Troubleshooting\n\n[It](Install,-update.md#update).\n")
        with open(self.root / "wiki/_Sidebar.md", "a", encoding="utf-8") as sidebar:
            sidebar.write("- [Install, update](user/Install,-update.md)\n")
        self.build()
        self.assertIn("[It](Install,-update#update).", self.page("Troubleshooting"))
        self.assertNotIn("# Install, update", self.page("Install,-update"))

    def test_the_sidebar_links_are_rewritten_and_it_has_no_footer(self) -> None:
        self.build()
        sidebar = self.page("_Sidebar")
        self.assertTrue(sidebar.startswith("<!-- Generated from wiki/_Sidebar.md."))
        self.assertIn("- [Install](Install#install)\n", sidebar)
        self.assertIn("- [Setup app](Setup-app)\n", sidebar)
        self.assertNotIn("generated from [", sidebar)

    def test_a_page_the_sidebar_leaves_out_fails_the_build(self) -> None:
        self.write("wiki/user/Extra.md", "# Extra\n")
        with self.assertRaisesRegex(wiki.BuildError, r"wiki/_Sidebar.md does not link wiki/user/Extra.md"):
            self.build()

    def test_a_missing_sidebar_fails_the_build(self) -> None:
        (self.root / "wiki/_Sidebar.md").unlink()
        with self.assertRaisesRegex(wiki.BuildError, "_Sidebar.md is missing"):
            self.build()

    def test_a_missing_home_page_fails_the_build(self) -> None:
        (self.root / "wiki/Home.md").unlink()
        with self.assertRaisesRegex(wiki.BuildError, "no Home.md"):
            self.build()

    def test_a_special_page_inside_a_folder_fails_the_build(self) -> None:
        self.write("wiki/user/_Footer.md", "Footer\n")
        with self.assertRaisesRegex(wiki.BuildError, "only at the top of wiki/"):
            self.build()

    def test_a_file_name_the_wiki_cannot_use_fails_the_build(self) -> None:
        for name in ("Setup in C#", "Setup app"):
            with self.subTest(name=name):
                path = self.root / f"wiki/developer/{name}.md"
                path.write_text("# Setup\n", encoding="utf-8")
                with self.assertRaisesRegex(wiki.BuildError, "a wiki file name holds no spaces"):
                    self.build()
                path.unlink()

    def test_links_to_other_repository_files_open_them_on_github(self) -> None:
        self.write("wiki/user/Troubleshooting.md", "# Troubleshooting\n\nRead [the crate](/crates/runtime).\n")
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
            "wiki/user/Troubleshooting.md",
            "# Troubleshooting\n\n`[a](Install.md)` and [b](Install.md)\n\n```md\n[c](Install.md)\n```\n",
        )
        self.build()
        page = self.page("Troubleshooting")
        self.assertIn("`[a](Install.md)` and [b](Install)", page)
        self.assertIn("```md\n[c](Install.md)\n```", page)

    def test_a_stray_backtick_hides_no_link_past_its_paragraph(self) -> None:
        self.write(
            "wiki/user/Troubleshooting.md", "# Troubleshooting\n\nPress the ` key.\n\nSee [it](Install.md) and `code`.\n"
        )
        self.build()
        self.assertIn("See [it](Install) and", self.page("Troubleshooting"))

    def test_an_image_inside_a_link_is_rewritten_with_it(self) -> None:
        self.write("wiki/user/Troubleshooting.md", "# Troubleshooting\n\n[![shot](images/window.png)](Install.md)\n")
        self.build()
        self.assertIn("[![shot](user/images/window.png)](Install)", self.page("Troubleshooting"))

    def test_an_image_elsewhere_in_the_repository_shows_its_raw_file(self) -> None:
        self.write("icon.png", "picture")
        self.write("wiki/user/Troubleshooting.md", "# Troubleshooting\n\n![icon](../../icon.png)\n")
        self.build()
        self.assertIn("![icon](https://github.com/owner/name/raw/main/icon.png)", self.page("Troubleshooting"))

    def test_a_link_to_a_wiki_folder_opens_it_on_github(self) -> None:
        self.write("wiki/user/Troubleshooting.md", "# Troubleshooting\n\nSee [the pages](../developer/).\n")
        self.build()
        self.assertIn("See [the pages](https://github.com/owner/name/tree/main/wiki/developer).", self.page("Troubleshooting"))

    def test_a_link_in_a_form_the_build_cannot_read_fails_it(self) -> None:
        self.write("wiki/user/Troubleshooting.md", "# Troubleshooting\n\nSee [it](Install.md 'Install').\n")
        with self.assertRaisesRegex(wiki.BuildError, "is in a form the wiki build cannot read"):
            self.build()

    def test_a_relative_html_link_fails_the_build(self) -> None:
        self.write("wiki/user/Troubleshooting.md", '# Troubleshooting\n\n<a href="Install.md">Install</a>\n')
        with self.assertRaisesRegex(wiki.BuildError, "is in a form the wiki build cannot read"):
            self.build()

    def test_parentheses_in_an_address_stay_in_it(self) -> None:
        self.write(
            "wiki/user/Troubleshooting.md",
            '# Troubleshooting\n\nSee [Foo](https://example.com/Foo_(bar)) and <a href="https://example.com">it</a>.\n',
        )
        self.build()
        self.assertIn("See [Foo](https://example.com/Foo_(bar)) and", self.page("Troubleshooting"))

    def test_footnotes_are_not_links(self) -> None:
        self.write("wiki/user/Troubleshooting.md", "# Troubleshooting\n\nAsk.[^1]\n\n[^1]: A note.\n")
        self.build()
        self.assertIn("[^1]: A note.\n", self.page("Troubleshooting"))

    def test_link_definitions_are_rewritten(self) -> None:
        self.write(
            "wiki/user/Troubleshooting.md", "# Troubleshooting\n\nSee [the guide][guide].\n\n[guide]: ../Home.md\n"
        )
        self.build()
        self.assertIn("[guide]: Home\n", self.page("Troubleshooting"))

    def test_a_heading_the_wiki_title_shows_is_dropped(self) -> None:
        self.build()
        self.assertTrue(self.page("Overview").startswith("<!-- Generated from wiki/user/Overview.md."))
        self.assertNotIn("# Overview", self.page("Overview"))
        self.assertIn("## Install\n", self.page("Install"))
        self.assertIn("# Example user guide", self.page("Home"))

    def test_each_page_links_the_file_to_edit(self) -> None:
        self.build()
        self.assertIn(
            "generated from [wiki/user/Install.md](https://github.com/owner/name/blob/main/wiki/user/Install.md)",
            self.page("Install"),
        )

    def test_a_link_to_a_missing_file_fails_the_build(self) -> None:
        self.write("wiki/user/Troubleshooting.md", "# Troubleshooting\n\nSee [gone](gone.md).\n")
        with self.assertRaisesRegex(wiki.BuildError, r"wiki/user/Troubleshooting.md: the link gone.md names no file"):
            self.build()

    def test_two_files_with_one_name_fail_the_build(self) -> None:
        self.write("wiki/developer/Overview.md", "# Overview\n")
        with self.assertRaisesRegex(wiki.BuildError, "both make the wiki page Overview"):
            self.build()

    def test_names_that_differ_only_in_case_fail_the_build(self) -> None:
        self.write("wiki/developer/OVERVIEW.md", "# Overview\n")
        with self.assertRaisesRegex(wiki.BuildError, "both make the wiki page"):
            self.build()


if __name__ == "__main__":
    unittest.main()
