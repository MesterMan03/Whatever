import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmodSync,
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";
import { tmpdir } from "node:os";
import { checkVersions } from "./check-versions.ts";

function run(command: string, args: string[]): string {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    windowsHide: true,
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.status !== 0)
    throw Error(`${command}: ${result.stdout ?? ""}${result.stderr ?? ""}`, {
      cause: result.error,
    });
  return result.stdout;
}

const xml = (value: string): string =>
  value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
const id = (prefix: string, path: string): string =>
  prefix + createHash("sha256").update(path.toLowerCase()).digest("hex").slice(0, 24);

function entries(root: string): string[] {
  return readdirSync(root, { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name))
    .flatMap((entry) => {
      const path = join(root, entry.name);
      if (entry.isSymbolicLink()) throw Error(`Installer source contains a symbolic link ${path}`);
      return entry.isDirectory() ? entries(path) : [path];
    });
}

export function windowsInstallerSource(source: string, version: string): string {
  const components: string[] = [];
  function directory(path: string): string {
    const local = relative(source, path).replaceAll("\\", "/");
    const directories = readdirSync(path, { withFileTypes: true }).sort((a, b) =>
      a.name.localeCompare(b.name),
    );
    const children = directories
      .map((entry) => {
        const file = join(path, entry.name);
        const key = relative(source, file).replaceAll("\\", "/");
        if (entry.isSymbolicLink()) throw Error("Installer source must contain ordinary files");
        if (entry.isDirectory()) return directory(file);
        const component = id("C", key);
        components.push(`<ComponentRef Id="${component}"/>`);
        return `<Component Id="${component}" Guid="*"><File Id="${id("F", key)}" Source="${xml(file)}" KeyPath="yes"/></Component>`;
      })
      .join("\n");
    if (!local) return children;
    return `<Directory Id="${local === "bin" ? "BINFOLDER" : id("D", local)}" Name="${xml(basename(path))}">${children}</Directory>`;
  }
  const files = directory(source);
  return `<?xml version="1.0" encoding="utf-8"?>
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs" xmlns:ui="http://wixtoolset.org/schemas/v4/wxs/ui">
  <Package UpgradeCode="DD68297A-0F4C-4414-9234-89A516A6C194" Name="Whatever Editor" Manufacturer="Whatever Engine" Version="${xml(version)}" Language="1033" Scope="perMachine">
    <MajorUpgrade AllowSameVersionUpgrades="yes" Schedule="afterInstallInitialize" DowngradeErrorMessage="A newer Whatever Editor version is already installed."/>
    <MediaTemplate EmbedCab="yes" CompressionLevel="high"/>
    <StandardDirectory Id="ProgramFiles64Folder"><Directory Id="INSTALLFOLDER" Name="Whatever Editor">${files}</Directory></StandardDirectory>
    <DirectoryRef Id="BINFOLDER">
      <Component Id="RegisterPath" Guid="*">
        <RegistryValue Root="HKLM" Key="Software\\WhateverEngine\\Editor" Name="PathRegistered" Type="integer" Value="1" KeyPath="yes"/>
        <Environment Id="EditorPath" Name="PATH" Value="[BINFOLDER]" Action="set" Part="last" System="yes" Permanent="no"/>
      </Component>
    </DirectoryRef>
    <StandardDirectory Id="ProgramMenuFolder"><Directory Id="EditorMenu" Name="Whatever Engine">
      <Component Id="EditorShortcut" Guid="*">
        <Shortcut Id="StartEditor" Name="Whatever Editor" Target="[#${id("F", "bin/whatever-editor.exe")}]" WorkingDirectory="INSTALLFOLDER"/>
        <RemoveFolder Id="RemoveEditorMenu" On="uninstall"/>
        <RegistryValue Root="HKLM" Key="Software\\WhateverEngine\\Editor" Name="ShortcutInstalled" Type="integer" Value="1" KeyPath="yes"/>
      </Component>
    </Directory></StandardDirectory>
    <Feature Id="Editor" Title="Whatever Editor" Level="1">${components.join("\n")}<ComponentRef Id="RegisterPath"/><ComponentRef Id="EditorShortcut"/></Feature>
    <ui:WixUI Id="WixUI_InstallDir" InstallDirectory="INSTALLFOLDER"/>
    <WixVariable Id="WixUILicenseRtf" Value="$(var.LicenseRtf)"/>
  </Package>
</Wix>`;
}

export function packageInstaller(source: string, output?: string): string {
  const root = resolve(source);
  const info = JSON.parse(readFileSync(join(root, "distribution.json"), "utf8"));
  const version = checkVersions();
  const platform =
    process.platform === "win32" ? "windows" : process.platform === "darwin" ? "macos" : "linux";
  const architecture = process.arch === "x64" ? "x86_64" : process.arch;
  if (
    info.version !== version ||
    info.platform !== platform ||
    info.architecture !== architecture ||
    info.format !== 2
  )
    throw Error("Installer source must be the matching portable distribution");
  const extension =
    process.platform === "win32" ? ".msi" : process.platform === "darwin" ? ".pkg" : ".deb";
  const artifact = resolve(output ?? root + extension);
  if (existsSync(artifact)) throw Error("Installer destination already exists");
  mkdirSync(dirname(artifact), { recursive: true });
  const staging = mkdtempSync(join(tmpdir(), "whatever-installer-"));
  try {
    if (process.platform === "win32") {
      const sourceFile = join(staging, "editor.wxs");
      writeFileSync(sourceFile, windowsInstallerSource(root, version));
      const license = readFileSync(join(root, "LICENSE"), "utf8")
        .replaceAll("\\", "\\\\")
        .replaceAll("{", "\\{")
        .replaceAll("}", "\\}")
        .replaceAll("\n", "\\par\n");
      const rtf = join(staging, "license.rtf");
      writeFileSync(
        rtf,
        "{\\rtf1\\ansi\\deff0 {\\fonttbl {\\f0 Arial;}}\\f0\\fs18\n" + license + "}",
      );
      const args = [
        "build",
        sourceFile,
        "-arch",
        process.arch === "arm64" ? "arm64" : "x64",
        "-ext",
        process.env.WIX_UI_EXTENSION ?? "WixToolset.UI.wixext",
        "-d",
        "LicenseRtf=" + rtf,
        "-intermediateFolder",
        join(staging, "obj"),
        "-pdbtype",
        "none",
        "-out",
        artifact,
      ];
      if (process.env.WIX_DLL)
        run(process.env.WIX_DOTNET ?? "dotnet", [process.env.WIX_DLL, ...args]);
      else run("wix", args);
    } else if (process.platform === "linux") {
      const tree = join(staging, "root");
      const installed = join(tree, "opt/whatever-editor");
      cpSync(root, installed, { recursive: true });
      mkdirSync(join(tree, "usr/bin"), { recursive: true });
      for (const command of ["whatever-editor", "wmod"])
        symlinkSync("/opt/whatever-editor/bin/" + command, join(tree, "usr/bin", command));
      mkdirSync(join(tree, "usr/share/applications"), { recursive: true });
      writeFileSync(
        join(tree, "usr/share/applications/whatever-editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=Whatever Editor\nExec=/opt/whatever-editor/bin/whatever-editor %F\nTerminal=false\nCategories=Development;Game;\n",
      );
      mkdirSync(join(tree, "usr/share/doc/whatever-editor"), { recursive: true });
      copyFileSync(join(root, "LICENSE"), join(tree, "usr/share/doc/whatever-editor/copyright"));
      mkdirSync(join(tree, "DEBIAN"), { recursive: true });
      const size = Math.ceil(
        entries(root).reduce((sum, file) => sum + statSync(file).size, 0) / 1024,
      );
      writeFileSync(
        join(tree, "DEBIAN/control"),
        `Package: whatever-editor\nVersion: ${version}\nArchitecture: ${process.arch === "arm64" ? "arm64" : "amd64"}\nMaintainer: Whatever Engine contributors\nSection: devel\nPriority: optional\nInstalled-Size: ${size}\nDepends: libc6 (>= 2.39), libgl1, libegl1, libvulkan1\nHomepage: https://github.com/MesterMan03/Whatever\nDescription: Whatever Engine editor and development tools\n Includes the embedded scripting VM, player, command-line tools and SDK.\n`,
      );
      run("dpkg-deb", ["--build", "--root-owner-group", tree, artifact]);
      run("dpkg-deb", ["--info", artifact]);
    } else {
      const tree = join(staging, "root");
      const app = join(root, "Whatever Editor.app");
      cpSync(app, join(tree, "Applications/Whatever Editor.app"), { recursive: true });
      const paths = join(tree, "etc/paths.d");
      mkdirSync(paths, { recursive: true });
      writeFileSync(
        join(paths, "whatever-engine"),
        "/Applications/Whatever Editor.app/Contents/MacOS\n",
      );
      chmodSync(join(paths, "whatever-engine"), 0o644);
      const components = join(staging, "components.plist");
      run("pkgbuild", ["--analyze", "--root", tree, components]);
      run("plutil", ["-replace", "0.BundleIsRelocatable", "-bool", "NO", components]);
      const args = [
        "--root",
        tree,
        "--component-plist",
        components,
        "--identifier",
        "engine.whatever.editor",
        "--version",
        version,
        "--install-location",
        "/",
        "--ownership",
        "recommended",
      ];
      if (process.env.WHATEVER_INSTALLER_IDENTITY)
        args.push("--sign", process.env.WHATEVER_INSTALLER_IDENTITY);
      run("pkgbuild", [...args, artifact]);
      run("pkgutil", ["--payload-files", artifact]);
    }
    console.log("Created " + artifact);
    return artifact;
  } catch (error) {
    if (existsSync(artifact)) rmSync(artifact);
    throw error;
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

if (import.meta.main) {
  const index = process.argv.indexOf("--source");
  if (index < 0 || !process.argv[index + 1])
    throw Error("Provide --source <portable editor directory>");
  const out = process.argv.indexOf("--output");
  packageInstaller(process.argv[index + 1], out < 0 ? undefined : process.argv[out + 1]);
}
