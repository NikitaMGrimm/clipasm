#!/usr/bin/env python3

from pathlib import Path
import unittest


REPOSITORY_ROOT = Path(__file__).parents[1]
WORKFLOW_PATH = REPOSITORY_ROOT / ".github" / "workflows" / "release.yml"


class ReleaseWorkflowTests(unittest.TestCase):
    def test_build_installs_ffmpeg_on_every_runner(self) -> None:
        workflow = WORKFLOW_PATH.read_text(encoding="utf-8")
        build_steps = workflow.split("\n  build:\n", maxsplit=1)[1].split(
            "\n  publish-crate:\n", maxsplit=1
        )[0]

        expected_installers = (
            ("Linux", "sudo apt-get update && sudo apt-get install -y ffmpeg"),
            ("macOS", "brew install ffmpeg"),
            ("Windows", "choco install ffmpeg --no-progress --yes"),
        )
        for runner, command in expected_installers:
            with self.subTest(runner=runner):
                self.assertIn(f"if: runner.os == '{runner}'", build_steps)
                self.assertIn(command, build_steps)


if __name__ == "__main__":
    unittest.main()
