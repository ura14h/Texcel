# Texcel

Texcel is a desktop app for editing the tables in Markdown files the way you would edit a spreadsheet. It also opens CSV/TSV files and imports tables from XLSX and HTML files into Markdown.

Texcel is written in Rust with [egui](https://github.com/emilk/egui) and runs on macOS, Windows, and Linux. Version: 0.2.1.

[日本語での概要](#日本語での概要)

## Features

- **Spreadsheet-style grid for Markdown tables.** Each table in a document gets its own tab at the bottom of the window. Text outside the tables and every unedited byte stay exactly as they were.
- **Live and source editing.** Live editing shows formula results and Markdown formatting (bold, italic, strikethrough, code, links). Source editing shows each cell's raw Markdown.
- **Formulas.** Cells starting with `=` are calculated with [IronCalc](https://github.com/ironcalc/IronCalc). Text such as `00123` or a 20-digit ID stays text, as in Excel: `=SUM()` ignores it and `=VALUE()` converts it.
- **CSV and TSV.** Delimiter, character encoding (including Shift_JIS / CP932), line endings, and quoting are kept when you save.
- **XLSX and HTML import.** Each sheet or table becomes a Markdown table. A report lists anything that could not be carried over, such as colors, borders, or merged cells. Texcel never writes back to XLSX or HTML.
- **Safe saving.** Files are written to a temporary file and then swapped in. If another app changed the file since you opened it, Texcel asks before overwriting. Unsaved work is restored after a crash.
- **One window per document**, light and dark themes, English and Japanese UI, and bundled Noto Sans CJK fonts for mixed Japanese and English text.

What Texcel keeps and loses for each format is described in [docs/basic-spec.md](docs/basic-spec.md) (section 9).

## Building

You need the stable Rust toolchain (edition 2024) and Cargo. The first build downloads the pinned dependencies, so it needs network access.

```sh
cargo build --locked --release --bin texcel
```

Run the app:

```sh
cargo run --locked --release --bin texcel
```

On Linux, file dialogs go through the XDG desktop portal, so a portal service (such as `xdg-desktop-portal-gtk` or `-kde`) must be running.

## Testing

```sh
cargo test --locked
cargo fmt --all --check
cargo clippy --locked --all-targets
```

The UI tests drive the real app through [egui_kittest](https://github.com/emilk/egui/tree/main/crates/egui_kittest). A few image tests need a GPU adapter and are marked `#[ignore]`.

## Packaging

### App Icon

`assets/icons/app_icon.png` (1024×1024) is the master icon. Regenerate the ICO, ICNS, and PNG files next to it with:

```sh
cargo run --locked --features icon-converter --bin texcel-icon-convert
```

### App Binary

| Platform | Command | Output |
| --- | --- | --- |
| macOS | `sh tools/package-macos.sh` | `target/package/mac/Texcel.app`, a universal binary for Apple silicon and Intel Macs, ad hoc signed for local use, with `licenses` beside it |
| Windows | `.\tools\package-windows.ps1` | `target/package/windows/Texcel.exe` with `licenses` beside it |
| Windows (from macOS or Linux) | `sh tools/package-windows.sh` | the same folder, cross-built for `x86_64-pc-windows-gnu` with MinGW-w64 |
| Linux | `sh tools/package-linux.sh` | `target/package/linux/usr`, a freedesktop tree with the desktop entry, icon, and `usr/share/doc/texcel/licenses` |

Each script builds the release binary itself with an explicit `--target`, so every release build lands in `target/<target-triple>/release` (for example `target/aarch64-apple-darwin/release` and `target/x86_64-pc-windows-gnu/release`); everyday `cargo build` and `cargo test` still use `target/debug`. `package-macos.sh` builds both `aarch64-apple-darwin` and `x86_64-apple-darwin`, joins them with `lipo`, and adds either Rust target with `rustup` when it is missing. `package-windows.sh` needs `x86_64-w64-mingw32-gcc` and `x86_64-w64-mingw32-windres` (for example from Homebrew's `mingw-w64`) and adds the Rust target with `rustup` when it is missing. On Windows, the build script embeds the icon using `rc.exe` (MSVC) or `windres` (GNU); set `RC` to use another resource compiler. Public distribution needs real code signing (and notarization on macOS).

### Release zips

Pushing a tag that is a semantic version (`v1.2.3`, `1.2.3`, or with a pre-release part such as `v1.2.3-rc.1`) runs `.github/workflows/package.yml`. The tag must match the version in `Cargo.toml` and `packaging/macos/Info.plist`; `tools/release-version.sh` checks this. The workflow builds `Texcel-mac-<version>.zip` and `Texcel-windows-<version>.zip` on macOS and attaches them to a GitHub Release named "Texcel <version>" with generated release notes. A tag with a pre-release part makes a pre-release, and re-running the workflow replaces the zips on the existing release. Each zip holds the app, its `licenses` folder, and `docs/user-manual.en.md` and `docs/user-manual.ja.md`. To build the same zips locally:

```sh
sh tools/package-zip.sh mac 0.2.1
sh tools/package-zip.sh windows 0.2.1
```

The zips are written to `target/dist/`.

## Project layout

| Path | Contents |
| --- | --- |
| `src/` | The application. Tests live next to the code in `*_test.rs` files. |
| `locales/` | English and Japanese UI text ([Fluent](https://projectfluent.org/)) |
| `assets/` | Icons and bundled fonts |
| `samples/` | Small input files and their expected results |
| `docs/` | The user manual ([English](docs/user-manual.en.md), [日本語](docs/user-manual.ja.md)) and the basic and detailed specifications (Japanese) |
| `tools/` | Icon converter, license bundler, and packaging scripts |
| `vendor/` | Patched copies of `ironcalc_base` and `umya-spreadsheet` |

## Status

Texcel 0.2.1 is an early release. Automated tests cover editing, formulas, file formats, saving, and undo. It has been built and run on macOS (arm64); the Intel part of the universal macOS app has only been started under Rosetta 2, not on an Intel Mac. On Windows, the release cross-built with MinGW-w64 has been started and has opened Markdown and XLSX files dropped onto its window, and the menu and shortcuts work in the new document windows. The Linux build, real Japanese IME input on each OS, and screen readers have not been checked yet. See section 12 of [docs/detail-spec.md](docs/detail-spec.md) for details.

## License

Texcel's own code is released under the [MIT License](LICENSE). That license covers only the code written for Texcel; everything below keeps the license its authors chose.

| Component | License | Notes |
| --- | --- | --- |
| [`vendor/ironcalc_base`](vendor/ironcalc_base) 0.9.1 ([IronCalc](https://github.com/ironcalc/IronCalc)) | MIT OR Apache-2.0 | Patched; see its [`PATCHES.md`](vendor/ironcalc_base/PATCHES.md). Contains Unicode CLDR data under the Unicode License v3 ([notice](vendor/ironcalc_base/THIRD-PARTY-NOTICES.md)). |
| [`vendor/umya-spreadsheet`](vendor/umya-spreadsheet) 3.1.1 | MIT | Patched; see its [`PATCHES.md`](vendor/umya-spreadsheet/PATCHES.md). |
| Noto Sans CJK JP and Noto Sans Mono CJK JP in [`assets/fonts/`](assets/fonts) | SIL Open Font License 1.1 | [License text](assets/fonts/LICENSE-Noto-Sans-CJK.txt) and [sources](assets/fonts/README.md). |
| egui's default fonts (`epaint_default_fonts`) | SIL Open Font License 1.1, Ubuntu Font Licence 1.0 | Inside the crate package. |
| All other crates (about 320 on macOS) | Mostly MIT and/or Apache-2.0; a few Zlib, BSD, ISC, BSL-1.0, Unicode-3.0, or Unlicense | Pinned in `Cargo.lock`. |

The two vendored crates keep their original license files, and the Texcel patches to them are offered under the same license as the crate they change, not under Texcel's license.

The packaging scripts run `texcel-licenses`, which puts Texcel's `LICENSE`, the Noto font license, and the license, notice, and patch files of every crate built into that platform's release into a `licenses` folder next to the app, one folder per crate. A few crates (such as egui) publish no license file; for those the folder holds a note with the license and authors from their `Cargo.toml`, and the standard MIT and Apache-2.0 texts are in `licenses/common/`. When a shipped crate carries the Boost Software License 1.0 text, it is copied there as well; the Windows release needs it for `clipboard-win`, which names only BSL-1.0. To create the folder on its own:

```sh
cargo run --locked --bin texcel-licenses -- target/package/licenses
```

## 日本語での概要

TexcelはMarkdownの表を表計算のように編集するデスクトップアプリです。CSV・TSVの読み書きと、XLSX・HTMLの表のMarkdownへの取り込みに対応します。表の外の文章と未編集の部分はバイト単位で保持し、ライブ編集では計算結果とMarkdownの修飾、ソース編集ではセルの原文を表示します。画面は日本語と英語に対応します。

使い方は [取扱説明書](docs/user-manual.ja.md)、仕様は [基本仕様書](docs/basic-spec.md) と [詳細設計書](docs/detail-spec.md) にあります。

Texcel本体のコードは [MITライセンス](LICENSE) です。`vendor/` に取り込んでパッチを当てたIronCalc・umya-spreadsheet、同梱のNotoフォント、その他の依存クレートは、それぞれ元のライセンスのままです。パッチは元のクレートと同じライセンスで提供し、配布物にはすべての依存クレートのライセンス文を同梱します。
