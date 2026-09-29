import contextlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from tools import git_hooks


class GitHookTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.root = self.directory / "repository with spaces"
        self.root.mkdir()
        self.bin = self.directory / "bin"
        self.bin.mkdir()
        self.log = self.directory / "just.log"
        environment = {
            key: value for key, value in os.environ.items() if not key.startswith("GIT_")
        }
        environment.update(
            GIT_CONFIG_NOSYSTEM="1",
            GIT_CONFIG_GLOBAL=str(self.directory / "global.gitconfig"),
            PATH=f"{self.bin}{os.pathsep}{os.environ['PATH']}",
            N3_HOOK_TEST_LOG=str(self.log),
        )
        self.environment = patch.dict(os.environ, environment, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.git("init", "-q", "--template=", "-b", "main")
        self.git("config", "user.name", "N3 hook test")
        self.git("config", "user.email", "hooks@example.invalid")
        for relative in (".githooks/pre-push", "tools/git_hooks.py"):
            destination = self.root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(git_hooks.ROOT / relative, destination)
        (self.root / ".gitignore").write_text(".cache/\n")
        (self.root / "tracked.txt").write_text("initial\n")
        self.git("add", ".")
        self.git("-c", "commit.gpgsign=false", "commit", "-qm", "Fixture")
        self.head = self.git("rev-parse", "HEAD")
        self.stub_just()

    def git(self, *args):
        result = subprocess.run(
            ["git", *args], cwd=self.root, capture_output=True, text=True, check=True
        )
        return result.stdout.strip()

    def stub_just(self, body="exit 0"):
        path = self.bin / "just"
        path.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$N3_HOOK_TEST_LOG"\n' + body + "\n")
        path.chmod(0o755)

    def refs(self, object_id=None, local="refs/heads/main"):
        return f"{local} {object_id or self.head} refs/heads/main {'0' * 40}\n"

    def invoke(self, command="pre-push", source=None):
        self.output = io.StringIO()
        with contextlib.redirect_stdout(self.output), contextlib.redirect_stderr(self.output):
            return git_hooks.main(
                [command], root=self.root,
                input_stream=io.StringIO(self.refs() if source is None else source),
            )

    def assert_not_verified(self):
        self.assertFalse(self.log.exists())

    def test_clean_checkout_runs_full_verify_once_for_multiple_head_refs(self):
        self.git("-c", "tag.gpgSign=false", "tag", "-am", "Fixture tag", "fixture-tag")
        tag = self.git("rev-parse", "fixture-tag")
        self.assertEqual(self.invoke(source=self.refs() + self.refs(tag, "refs/tags/fixture-tag")), 0)
        self.assertEqual(self.log.read_text(), "verify\n")
        self.assertEqual(self.git("status", "--porcelain"), "")

    def test_dirty_tracked_changes_are_rejected_before_verification(self):
        (self.root / "tracked.txt").write_text("changed\n")
        self.assertEqual(self.invoke(), 1)
        self.assertIn("clean worktree", self.output.getvalue())
        self.assert_not_verified()

    def test_staged_changes_are_rejected_before_verification(self):
        (self.root / "tracked.txt").write_text("changed\n")
        self.git("add", "tracked.txt")
        self.assertEqual(self.invoke(), 1)
        self.assert_not_verified()

    def test_nonignored_untracked_files_are_rejected(self):
        (self.root / "new.txt").write_text("not committed\n")
        self.assertEqual(self.invoke(), 1)
        self.assert_not_verified()

    def test_ignored_build_output_does_not_block_verification(self):
        (self.root / ".cache").mkdir()
        (self.root / ".cache/output").write_text("build output\n")
        self.assertEqual(self.invoke(), 0)
        self.assertEqual(self.log.read_text(), "verify\n")

    def test_non_head_push_is_rejected_even_alongside_head(self):
        previous = self.head
        self.git("-c", "commit.gpgsign=false", "commit", "--allow-empty", "-qm", "Next")
        self.head = self.git("rev-parse", "HEAD")
        self.assertEqual(self.invoke(source=self.refs() + self.refs(previous, "refs/heads/old")), 1)
        self.assertIn("does not point", self.output.getvalue())
        self.assert_not_verified()

    def test_deletions_and_empty_input_need_no_clean_checkout_or_verification(self):
        (self.root / "tracked.txt").write_text("changed\n")
        for source in ("", self.refs("0" * 40, "(delete)")):
            with self.subTest(source=source):
                self.assertEqual(self.invoke(source=source), 0)
                self.assert_not_verified()

    def test_mixed_deletion_and_head_push_still_verifies(self):
        self.assertEqual(self.invoke(source=self.refs("0" * 40, "(delete)") + self.refs()), 0)
        self.assertEqual(self.log.read_text(), "verify\n")

    def test_malformed_ref_input_fails_before_verification(self):
        for source in ("main\n", self.refs("--help"), self.refs() + "bad input\n"):
            with self.subTest(source=source):
                self.assertEqual(self.invoke(source=source), 1)
                self.assertIn("Malformed", self.output.getvalue())
                self.assert_not_verified()

    def test_non_commit_target_fails_before_verification(self):
        blob = self.git("rev-parse", "HEAD:tracked.txt")
        self.assertEqual(self.invoke(source=self.refs(blob, "refs/tags/blob")), 1)
        self.assert_not_verified()

    def test_verify_failure_is_propagated(self):
        self.stub_just("exit 7")
        self.assertEqual(self.invoke(), 7)
        self.assertEqual(self.log.read_text(), "verify\n")

    def test_changes_created_during_verify_are_rejected(self):
        self.stub_just("printf changed > tracked.txt")
        self.assertEqual(self.invoke(), 1)
        self.assertIn("clean worktree", self.output.getvalue())

    def test_head_change_during_verify_is_rejected(self):
        self.stub_just("git -c commit.gpgsign=false commit --allow-empty -qm Moved")
        self.assertEqual(self.invoke(), 1)
        self.assertIn("HEAD changed", self.output.getvalue())

    def test_install_is_local_and_idempotent(self):
        self.assertEqual(self.invoke("install"), 0)
        self.assertEqual(self.invoke("install"), 0)
        self.assertEqual(self.git("config", "--local", "--get-all", "core.hooksPath"), ".githooks")
        self.assertFalse((self.directory / "global.gitconfig").exists())

    def test_install_refuses_local_and_inherited_hooks_path_conflicts(self):
        for scope in ("--local", "--global"):
            with self.subTest(scope=scope):
                self.git("config", scope, "core.hooksPath", "custom-hooks")
                self.assertEqual(self.invoke("install"), 1)
                self.assertIn("already points elsewhere", self.output.getvalue())
                self.assertEqual(self.git("config", "--get", "core.hooksPath"), "custom-hooks")
                self.git("config", scope, "--unset", "core.hooksPath")

    def test_install_preserves_an_active_existing_pre_push(self):
        hook = self.root / ".git/hooks/pre-push"
        hook.parent.mkdir()
        hook.write_text("#!/bin/sh\nexit 0\n")
        hook.chmod(0o755)
        self.assertEqual(self.invoke("install"), 1)
        self.assertIn("already exist: pre-push", self.output.getvalue())
        self.assertEqual(hook.read_text(), "#!/bin/sh\nexit 0\n")
        self.assertIsNone(git_hooks.git(self.root, "config", "--get", "core.hooksPath", missing_ok=True))

    def test_install_preserves_other_active_default_hooks(self):
        directory = self.root / ".git/hooks"
        directory.mkdir()
        for name in ("pre-commit", "commit-msg"):
            hook = directory / name
            hook.write_text("#!/bin/sh\nexit 0\n")
            hook.chmod(0o755)
        self.assertEqual(self.invoke("install"), 1)
        for name in ("pre-commit", "commit-msg"):
            self.assertIn(name, self.output.getvalue())
            self.assertEqual((directory / name).read_text(), "#!/bin/sh\nexit 0\n")
        self.assertIsNone(git_hooks.git(self.root, "config", "--get", "core.hooksPath", missing_ok=True))

    def test_install_allows_inactive_default_hooks_and_samples(self):
        directory = self.root / ".git/hooks"
        directory.mkdir()
        for name, mode in (("pre-commit", 0o644), ("pre-push.sample", 0o755)):
            hook = directory / name
            hook.write_text("#!/bin/sh\nexit 0\n")
            hook.chmod(mode)
        self.assertEqual(self.invoke("install"), 0)
        self.assertEqual(self.git("config", "--local", "--get", "core.hooksPath"), ".githooks")

    def test_install_rejects_missing_or_nonexecutable_repository_hook(self):
        hook = self.root / ".githooks/pre-push"
        hook.chmod(0o644)
        self.assertEqual(self.invoke("install"), 1)
        hook.unlink()
        self.assertEqual(self.invoke("install"), 1)
        self.assertIsNone(git_hooks.git(self.root, "config", "--get", "core.hooksPath", missing_ok=True))

    def test_installed_shell_hook_uses_repo_root_and_preserves_stdin(self):
        self.assertEqual(self.invoke("install"), 0)
        result = subprocess.run(
            [str(self.root / ".githooks/pre-push"), "origin", "unused-remote"],
            cwd=self.root / "tools", input=self.refs(), capture_output=True, text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.log.read_text(), "verify\n")


if __name__ == "__main__":
    unittest.main()
