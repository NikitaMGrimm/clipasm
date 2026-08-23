#!/usr/bin/env python3

from pathlib import Path
import unittest


REPOSITORY_ROOT = Path(__file__).parents[1]
WORKFLOW_DIRECTORY = REPOSITORY_ROOT / ".github" / "workflows"
CI_WORKFLOW_PATH = WORKFLOW_DIRECTORY / "ci.yml"
RELEASE_WORKFLOW_PATH = WORKFLOW_DIRECTORY / "release.yml"


class ReleaseWorkflowTests(unittest.TestCase):
    def test_release_tag_commit_must_belong_to_origin_main(self) -> None:
        workflow = RELEASE_WORKFLOW_PATH.read_text(encoding="utf-8")

        tag_commit_check = (
            "git fetch --no-tags origin "
            "+refs/heads/main:refs/remotes/origin/main\n"
            "          git merge-base --is-ancestor HEAD origin/main"
        )
        self.assertIn(tag_commit_check, workflow)
        self.assertLess(
            workflow.index(tag_commit_check), workflow.index("cargo publish --dry-run")
        )

    def test_release_checks_both_public_api_surfaces(self) -> None:
        workflow = RELEASE_WORKFLOW_PATH.read_text(encoding="utf-8")

        self.assertIn("cargo-semver-checks@0.49.0", workflow)
        self.assertIn(
            "cargo semver-checks check-release --default-features",
            workflow,
        )
        self.assertIn(
            "cargo semver-checks check-release --only-explicit-features",
            workflow,
        )

    def test_build_installs_ffmpeg_on_every_runner(self) -> None:
        workflow = RELEASE_WORKFLOW_PATH.read_text(encoding="utf-8")
        build_steps = workflow.split("\n  build:\n", maxsplit=1)[1].split(
            "\n  publish-crate:\n", maxsplit=1
        )[0]

        expected_installers = (
            ("Linux", "sudo apt-get update && sudo apt-get install -y ffmpeg"),
            ("macOS", "brew install ffmpeg-full"),
            (
                "Windows",
                "choco install ffmpeg --no-progress --yes "
                "--source=https://community.chocolatey.org/api/v2/",
            ),
        )
        for runner, command in expected_installers:
            with self.subTest(runner=runner):
                self.assertIn(f"if: runner.os == '{runner}'", build_steps)
                self.assertIn(command, build_steps)

        self.assertIn('echo "$(brew --prefix ffmpeg-full)/bin" >> "$GITHUB_PATH"', build_steps)
        self.assertIn("Get-Command ffmpeg -ErrorAction SilentlyContinue", build_steps)
        self.assertIn("Get-Command ffprobe -ErrorAction SilentlyContinue", build_steps)
        self.assertIn('$filters -match "(?m)\\bzscale\\b"', build_steps)


class WorkflowConfigurationTests(unittest.TestCase):
    def test_ci_runs_for_main_pushes_and_pull_requests_once(self) -> None:
        workflow = CI_WORKFLOW_PATH.read_text(encoding="utf-8")

        self.assertIn("  push:\n    branches: [main]\n  pull_request:", workflow)

    def test_ci_cancels_superseded_runs(self) -> None:
        workflow = CI_WORKFLOW_PATH.read_text(encoding="utf-8")

        self.assertIn(
            "concurrency:\n"
            "  group: ci-${{ github.workflow }}-"
            "${{ github.event.pull_request.number || github.ref }}\n"
            "  cancel-in-progress: true",
            workflow,
        )

    def test_workflows_use_pinned_prebuilt_tools(self) -> None:
        workflows = "\n".join(
            path.read_text(encoding="utf-8")
            for path in sorted(WORKFLOW_DIRECTORY.glob("*.yml"))
        )

        self.assertNotIn("cargo install ", workflows)
        self.assertIn(
            "taiki-e/install-action@18b1216eba7f8039b0f8d131d5473787f0edce68",
            workflows,
        )
        self.assertNotIn("\n          toolchain:", workflows)


if __name__ == "__main__":
    unittest.main()
