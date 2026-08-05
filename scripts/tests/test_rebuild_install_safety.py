#!/usr/bin/env python3


"""Regression tests for user-install ownership and symlink safety."""


from __future__ import annotations


import pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "tools"))
import os
from pathlib import Path
import json
import shutil
import stat
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
INSTALLER = ROOT / "scripts" / "rebuild-install-relaunch.sh"
WINDOWS_INSTALLER = ROOT / "scripts" / "rebuild-install-relaunch.ps1"
MARKER = "optimus-agent-user-install-v1:test-fixture\n"


def fake_binary(path: Path, name: str, version: str = "0.1.0") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f"#!/bin/sh\necho '{name} {version}'\n", encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR)


class LinuxInstallerSafetyTest(unittest.TestCase):
    def test_linux_installer_requires_bubblewrap_for_runtime_containment(self) -> None:
        script = INSTALLER.read_text(encoding="utf-8")
        self.assertIn("require_command bwrap", script)

    def run_installer(
        self,
        root: Path,
        *,
        binary_version: str = "0.1.0",
        desktop_script: str | None = None,
        env_overrides: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        data = root / "data"
        bin_home = root / "user-bin"
        install = data / "optimus-agent"
        target = root / "target"
        fake_binary(
            target / "release" / "optimus-desktop",
            "optimus-desktop",
            binary_version,
        )
        fake_binary(
            target / "release" / "optimus-agent",
            "optimus-agent",
            binary_version,
        )
        if desktop_script is not None:
            tauri = target / "release" / "optimus-agent"
            tauri.write_text(desktop_script, encoding="utf-8")
            tauri.chmod(tauri.stat().st_mode | stat.S_IXUSR)
        fake_binary(target / "release" / "optimus", "optimus", binary_version)
        env = os.environ.copy()
        env.update(
            {
                "XDG_DATA_HOME": str(data),
                "XDG_BIN_HOME": str(bin_home),
                "OPTIMUS_INSTALL_ROOT": str(install),
                "CARGO_TARGET_DIR": str(target),
            }
        )
        env.update(env_overrides or {})
        return subprocess.run(
            ["bash", str(INSTALLER), "--no-build", "--no-relaunch"],
            cwd=ROOT,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=30,
            check=False,
        )

    def test_owned_install_rejects_symlinked_bin_directory(self) -> None:
        with tempfile.TemporaryDirectory(prefix="optimus-installer-safety-") as tmp:
            root = Path(tmp)
            install = root / "data" / "optimus-agent"
            outside = root / "outside"
            install.mkdir(parents=True)
            outside.mkdir()
            (install / ".optimus-agent-install").write_text(MARKER, encoding="utf-8")
            (install / "bin").symlink_to(outside, target_is_directory=True)

            result = self.run_installer(root)

            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("symlink", result.stdout.lower())
            self.assertFalse((outside / "optimus-desktop").exists())
            self.assertFalse((outside / "optimus").exists())

    def test_install_rejects_symlinked_root_before_canonicalization(self) -> None:
        with tempfile.TemporaryDirectory(prefix="optimus-installer-root-link-") as tmp:
            root = Path(tmp)
            install = root / "data" / "optimus-agent"
            outside = root / "outside"
            install.parent.mkdir(parents=True)
            outside.mkdir()
            (outside / ".optimus-agent-install").write_text(MARKER, encoding="utf-8")
            install.symlink_to(outside, target_is_directory=True)

            result = self.run_installer(root)

            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("symlinked install root", result.stdout.lower())
            self.assertFalse((outside / "bin" / "optimus-desktop").exists())
            self.assertFalse((outside / "bin" / "optimus").exists())

    def test_install_rejects_symlinked_application_directory_component(self) -> None:
        with tempfile.TemporaryDirectory(prefix="optimus-installer-app-link-") as tmp:
            root = Path(tmp)
            data = root / "data"
            outside = root / "outside-applications"
            data.mkdir()
            outside.mkdir()
            (data / "applications").symlink_to(outside, target_is_directory=True)

            result = self.run_installer(root)

            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("symlink", result.stdout.lower())
            self.assertFalse((outside / "optimus-agent.desktop").exists())

    def test_install_rejects_parent_traversal_that_hides_a_symlink(self) -> None:
        with tempfile.TemporaryDirectory(prefix="optimus-installer-dotdot-link-") as tmp:
            root = Path(tmp)
            outside = root / "outside"
            outside.mkdir()
            (root / "link").symlink_to(outside, target_is_directory=True)
            escaped = root / "missing" / ".." / "link"

            result = self.run_installer(
                root,
                env_overrides={
                    "XDG_DATA_HOME": str(escaped),
                    "OPTIMUS_INSTALL_ROOT": str(escaped / "optimus-agent"),
                },
            )

            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("parent traversal", result.stdout.lower())
            self.assertFalse((outside / "optimus-agent" / "bin").exists())

    def test_install_rejects_symlinked_bin_home_component(self) -> None:
        with tempfile.TemporaryDirectory(prefix="optimus-installer-bin-link-") as tmp:
            root = Path(tmp)
            outside = root / "outside-bin"
            outside.mkdir()
            (root / "user-bin").symlink_to(outside, target_is_directory=True)

            result = self.run_installer(root)

            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("symlink", result.stdout.lower())
            self.assertFalse((outside / "optimus").exists())
            self.assertFalse((outside / "optimus-cli").exists())

    def test_no_build_rejects_binary_version_that_differs_from_policy(self) -> None:
        with tempfile.TemporaryDirectory(prefix="optimus-installer-version-") as tmp:
            root = Path(tmp)

            result = self.run_installer(root, binary_version="0.19.0+foreign")

            install = root / "data" / "optimus-agent"
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("does not match policy version", result.stdout)
            self.assertFalse((install / ".optimus-agent-install").exists())
            self.assertFalse((install / "bin" / "optimus-desktop").exists())
            self.assertFalse((install / "bin" / "optimus").exists())

    def test_release_policy_is_rechecked_after_binary_selection(self) -> None:
        with tempfile.TemporaryDirectory(prefix="optimus-installer-second-gate-") as tmp:
            root = Path(tmp)
            shim_dir = root / "shim"
            shim_dir.mkdir()
            call_log = root / "python-calls.log"
            real_python = shutil.which("python3")
            self.assertIsNotNone(real_python)
            shim = shim_dir / "python3"
            shim.write_text(
                "#!/bin/sh\n"
                "printf '%s\\n' \"$*\" >>\"$PYTHON_CALL_LOG\"\n"
                f"exec {real_python} \"$@\"\n",
                encoding="utf-8",
            )
            shim.chmod(shim.stat().st_mode | stat.S_IXUSR)

            result = self.run_installer(
                root,
                env_overrides={
                    "PATH": f"{shim_dir}:{os.environ['PATH']}",
                    "PYTHON_CALL_LOG": str(call_log),
                },
            )

            self.assertEqual(result.returncode, 0, result.stdout)
            release_checks = [
                line
                for line in call_log.read_text(encoding="utf-8").splitlines()
                if "scripts/tools/optimus_version.py release-check" in line
            ]
            self.assertEqual(release_checks, [release_checks[0], release_checks[0]])

    def test_no_build_installs_tauri_primary_without_electron(self) -> None:
        with tempfile.TemporaryDirectory(prefix="optimus-installer-tauri-") as tmp:
            root = Path(tmp)

            result = self.run_installer(root)

            self.assertEqual(result.returncode, 0, result.stdout)
            install = root / "data" / "optimus-agent"
            launcher = install / "bin" / "optimus-desktop"
            tauri = install / "bin" / "optimus-agent-tauri"
            host = install / "bin" / "optimus-desktop-host"
            desktop_entry = root / "data" / "applications" / "optimus-agent.desktop"

            self.assertTrue(launcher.is_file())
            self.assertTrue(os.access(launcher, os.X_OK))
            self.assertTrue(tauri.is_file())
            self.assertTrue(os.access(tauri, os.X_OK))
            self.assertFalse(host.exists())
            self.assertFalse((install / "app-bundle").exists())
            launcher_source = launcher.read_text(encoding="utf-8")
            self.assertIn("GDK_BACKEND", launcher_source)
            self.assertIn("WEBKIT_DISABLE_COMPOSITING_MODE", launcher_source)
            self.assertNotIn("ELECTRON", launcher_source)
            self.assertNotIn("OPTIMUS_DESKTOP_SHELL", launcher_source)
            self.assertNotIn("LegacyWry", launcher_source)
            self.assertNotIn("wry", launcher_source)
            self.assertEqual(
                subprocess.check_output(
                    [str(launcher), "--version"],
                    text=True,
                ).strip(),
                "optimus-desktop 0.1.0",
            )
            self.assertEqual(
                subprocess.check_output([str(tauri), "--version"], text=True).strip(),
                "optimus-agent 0.1.0",
            )
            entry = desktop_entry.read_text(encoding="utf-8")
            self.assertIn(f'Exec="{launcher}"', entry)
            self.assertIn("X-Optimus-UI=react-tauri", entry)
            self.assertNotIn("ElectronRollback", entry)
            self.assertNotIn("LegacyWry", entry)
            self.assertNotIn("OPTIMUS_DESKTOP_SHELL", entry)
            self.assertNotIn("wry", entry)
            version_txt = (install / "VERSION.txt").read_text(encoding="utf-8")
            self.assertIn("shell=react-tauri", version_txt)
            self.assertNotIn("electron", version_txt)
            install_meta = json.loads(
                (install / "install-meta.json").read_text(encoding="utf-8")
            )
            self.assertNotIn("host_binary", install_meta)
            self.assertEqual(install_meta["desktop_shell"], "react-tauri")

    def test_reinstall_prunes_stale_pre_tauri_app_bundle(self) -> None:
        with tempfile.TemporaryDirectory(prefix="optimus-installer-prune-") as tmp:
            root = Path(tmp)
            install = root / "data" / "optimus-agent"

            # Simulate a pre-Tauri owned install that still carries an
            # app-bundle/ runtime staged by the retired product.
            first = self.run_installer(root)
            self.assertEqual(first.returncode, 0, first.stdout)
            stale = install / "app-bundle" / "electron"
            stale.mkdir(parents=True)
            stale_runtime = stale / "optimus-agent"
            stale_runtime.write_text("stale runtime", encoding="utf-8")
            (install / ".optimus-agent-install").write_text(
                MARKER, encoding="utf-8"
            )
            self.assertTrue(stale_runtime.is_file())

            second = self.run_installer(root)

            self.assertEqual(second.returncode, 0, second.stdout)
            self.assertFalse((install / "app-bundle").exists())
            self.assertIn("Pruning stale pre-Tauri app-bundle", second.stdout)

    def test_install_rejects_binary_changed_after_first_version_validation(self) -> None:
        with tempfile.TemporaryDirectory(prefix="optimus-installer-artifact-race-") as tmp:
            root = Path(tmp)
            script = (
                "#!/bin/sh\n"
                'count_file="$0.version-count"\n'
                'count=$(cat "$count_file" 2>/dev/null || printf 0)\n'
                'count=$((count + 1))\n'
                'printf "%s\\n" "$count" > "$count_file"\n'
                "printf 'optimus-agent 0.1.0\\n'\n"
                'if [ "$count" -ge 2 ]; then\n'
                "  printf '#!/bin/sh\\nprintf malicious\\n' > \"$0\"\n"
                '  chmod 755 "$0"\n'
                "fi\n"
            )

            result = self.run_installer(root, desktop_script=script)

            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("changed after version validation", result.stdout)
            self.assertFalse(
                (root / "data" / "optimus-agent" / "bin" / "optimus-desktop").exists()
            )
            self.assertFalse(
                (root / "data" / "optimus-agent" / "bin" / "optimus-agent-tauri").exists()
            )


class WindowsInstallerSafetyContractTest(unittest.TestCase):
    def test_windows_installer_is_owned_reparse_safe_and_portable(self) -> None:
        script = WINDOWS_INSTALLER.read_text(encoding="utf-8")

        self.assertIn(".optimus-agent-install", script)
        self.assertIn("Assert-InstallRootOwnership", script)
        self.assertGreaterEqual(script.count("Assert-InstallRootOwnership"), 3)
        self.assertIn("ReparsePoint", script)
        self.assertIn("expectedRoot", script)
        self.assertIn("ownership marker", script)
        self.assertIn("builtVersion", script)
        self.assertIn("product_version", script)
        self.assertIn("Assert-NoReparseComponents", script)
        self.assertIn("Assert-ShortcutOwned", script)
        self.assertIn("GetFileInformationByHandle", script)
        self.assertIn("Assert-SingleLink", script)
        self.assertIn("[IO.FileMode]::CreateNew", script)
        self.assertIn("[Guid]::NewGuid", script)
        self.assertGreaterEqual(script.count("scripts/tools/optimus_version.py release-check"), 2)
        self.assertGreaterEqual(script.count("Get-FileHash"), 4)
        self.assertIn("ExpectedSha256", script)
        self.assertNotIn("$tmp = $dest + '.new'", script)
        self.assertNotIn("$tmp = $Path + '.new'", script)
        self.assertNotIn("C:\\Users\\mustb", script)
        self.assertNotIn("$version = '0.1.0'", script)


if __name__ == "__main__":
    unittest.main()
