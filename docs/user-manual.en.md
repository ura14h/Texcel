# Texcel User Manual

Version: 0.2.1

This manual covers starting Texcel from its release package, editing tables, and saving them. [日本語版](user-manual.ja.md)

## 1. What Texcel does

Texcel edits Markdown tables the way a spreadsheet edits cells. You see cell values, formula results, and Markdown formatting such as bold text and links in the same table while you edit.

| File | Opening | Saving |
| --- | --- | --- |
| Markdown (.md, .markdown) | Every table in the document is editable | Text outside the tables and untouched parts stay as they were |
| CSV, TSV (.csv, .tsv) | One table | Keeps the original delimiter, encoding, and line endings |
| XLSX (.xlsx) | Each sheet is imported as a Markdown table | Not saved back to XLSX |
| HTML (.html, .htm) | Each table is imported as a Markdown table | Not saved back to HTML |

Texcel works only with files on your computer. It needs no account and no network connection.

## 2. Requirements

- macOS, Windows, or Linux.
- Texcel draws its window with the GPU. If no usable GPU driver is available, Texcel shows an error at startup and does not open.
- On Linux, file dialogs need the XDG desktop portal (such as `xdg-desktop-portal-gtk` or `xdg-desktop-portal-kde`).
- The interface is in English or Japanese. At first it follows the system language.

## 3. Package contents and starting Texcel

The macOS and Windows versions come as `Texcel-mac-<version>.zip` and `Texcel-windows-<version>.zip`. Each zip holds the app, a `licenses` folder, and this manual in English and Japanese. The `licenses` folder has the license texts of Texcel and of the libraries and fonts it includes.

### 3.1 macOS

Double-click `Texcel-mac-<version>.zip` to extract it. It contains:

```
Texcel-mac-<version>/
├── Texcel.app
├── licenses/
├── user-manual.en.md
└── user-manual.ja.md
```

1. Copy `Texcel.app` to the Applications folder.
2. Double-click `Texcel.app`.

This version runs on Macs with Apple silicon (M1 or later) and on Intel Macs.

This package is not notarized. If you copied or downloaded it from another Mac, macOS may say that it cannot open Texcel the first time. Open System Settings > Privacy & Security, select **Open Anyway** next to the message about Texcel, and start Texcel again.

You can also right-click a file in Finder and choose Open With > Texcel.

### 3.2 Windows

Right-click `Texcel-windows-<version>.zip` and choose Extract All. It contains:

```
Texcel-windows-<version>\
├── Texcel.exe
├── licenses\
├── user-manual.en.md
└── user-manual.ja.md
```

1. Put the extracted folder where you like, such as `C:\Program Files\Texcel` or a folder under your Documents.
2. Double-click `Texcel.exe`.

This package is not code-signed. If Windows shows "Windows protected your PC" the first time, select **More info**, then **Run anyway**.

To open a file with Texcel, right-click it in File Explorer, choose Open with > Choose another app > Choose an app on your PC, and select `Texcel.exe`.

### 3.3 Linux

Package contents (freedesktop layout):

```
linux/
└── usr/
    ├── bin/texcel
    └── share/
        ├── applications/net.homeunix.hio.app.Texcel.desktop
        ├── icons/hicolor/256x256/apps/texcel.png
        └── doc/texcel/licenses/
```

For your user account only, copy the contents of `usr` into `~/.local`:

```bash
cp -R linux/usr/. ~/.local/
```

For all users, copy them into `/usr/local`:

```bash
sudo cp -R linux/usr/. /usr/local/
```

Then choose Texcel from the application list, or run `texcel` in a terminal. When you install into `~/.local`, `~/.local/bin` must be on your `PATH`.

### 3.4 Opening a file at startup

Each of these opens a file the same way as Open…:

- Drag a file onto a Texcel window.
- Choose File > Open Recent.
- Pass the file on the command line, as in `texcel file.md` (`Texcel.exe file.md` on Windows).
- Choose Texcel from the system's Open With menu.

## 4. The window

Texcel starts with an empty sheet (Table1). You can type into any cell right away.

| Area | What it shows |
| --- | --- |
| Menus | File, Edit, View, Insert, Format, Data, and Help. On macOS they are in the menu bar at the top of the screen, with an extra Window menu. On Windows they are in the window's menu bar (also reachable with Alt). On Linux they are inside the window |
| Name box | The address of the selected cell, such as `B2`. Type `C12` or `A1:D20` and press Enter to go to that cell or range |
| ✗ ✓ fx | ✗ cancels the entry, ✓ confirms it, and fx inserts a function. ✗ and ✓ work only while you are entering text |
| Formula bar | The selected cell's source, including formulas and Markdown marks. Click it to edit from that point |
| Grid | Cells with column letters (A, B, …) and row numbers (1, 2, …) |
| Table tabs | Switch between the tables in the document. **+** adds a table |
| List | Picks a table from a list. Each entry shows the table name and the content of the filled cell nearest to A1 |
| Live Editing / Source Editing | Switches the display mode. The button shows the current mode |
| Status bar | Problems and warnings you need to act on, progress with a Cancel button, and how many cells are selected |

Each window holds one document. New Document and Open… open a new window. If the file is already open, Texcel brings its window to the front instead. A document with unsaved changes is marked in the window title (on macOS, a dot in the close button).

### 4.1 Live Editing and Source Editing

- **Live Editing** (default) shows formula results and renders Markdown formatting such as bold, italic, and links.
- **Source Editing** shows the Markdown source of every cell, including formulas and marks such as `**`.

Switch with the button at the bottom right, the View menu, or Ctrl+/ (Cmd+/ on macOS). Switching does not change the document.

## 5. Basic operations

Below, "Ctrl/Cmd" means Ctrl on Windows and Linux and Cmd on macOS.

### 5.1 Selecting and moving

- Click a cell or use the arrow keys.
- Select a range with Shift+arrow keys, by dragging, or with Shift+click. Ctrl/Cmd+click adds another range.
- Click a column letter or row number to select the whole column or row.
- Ctrl/Cmd+arrow key moves to the edge of the block of filled cells.
- Type an address or range in the name box to go there.

### 5.2 Entering and editing

- Select a cell and start typing to replace its content. Japanese input (IME) starts the same way.
- To change the existing content, press F2 or double-click the cell. You can also click the formula bar.
- Enter confirms and moves down; Tab confirms and moves right. Add Shift to move the other way. ✓ in the formula bar confirms without moving.
- Esc or ✗ in the formula bar cancels the entry and restores the previous content.
- While you edit in the formula bar, right-click the text to Undo, Redo, Cut, Copy, Paste, or Select All.
- Alt+Enter starts a new line inside the cell (saved as `<br>`).
- Delete or Backspace clears the selected cells.

When you start typing on a selected cell, the arrow keys confirm the entry and move to the next cell. When you start with F2 or a double-click, the arrow keys move the text cursor.

### 5.3 Formulas

Content that starts with `=` is calculated as a formula, such as `=B2*C2` or `=SUM(D2:D3)`.

- While entering a formula, click or drag over cells to insert their addresses. Inserted references are outlined in color.
- F2 switches between moving the text cursor and picking references.
- F4 (Cmd+T on macOS) cycles a reference through relative, absolute, row-locked, and column-locked.
- **fx** in the formula bar opens the function list. Type part of a name to filter it, then choose a function. Texcel inserts `=NAME()` with the cursor between the parentheses, so you can keep typing or click and drag over cells to add a range.
- To store text that starts with `=` as a value, put `'` in front of it: `'=A1` is the text "=A1".
- Texcel does not turn text such as `00123`, 20-digit IDs, or date-like text into numbers. Sums and other totals skip such text. Use `=VALUE()` to calculate with it as a number.

To replace formulas with their results, choose Data > Replace formulas in selection with results.

### 5.4 Fill

Drag the small square at the bottom right of the selection (the fill handle) to fill the cells you drag over.

- If two or more values in one row or column form an even sequence of numbers, dates, or text ending in a number (such as `Item1`, `Item2`), Texcel continues the sequence.
- Anything else (a single cell, formulas, values without a clear sequence) is copied.

Edit > Fill lets you choose filling by copying or with a series explicitly. Ctrl+D and Ctrl+R copy the cell above or to the left.

### 5.5 Rows, columns, and tables

- Insert rows and columns from the Insert menu. Delete, duplicate, and move them from Edit > Rows and Edit > Columns. The same commands are on the right-click menus of cells, row numbers, and column letters.
- Drag the border between column letters to change a column's width, or double-click it to fit the content. Column widths are a view setting and are not saved in the file.
- Add, duplicate, and delete tables from Edit > Tables or from a table tab's right-click menu. A CSV document holds only one table.

### 5.6 Markdown formatting

The Format menu and these shortcuts add or remove formatting on the selected cells.

| Formatting | Shortcut |
| --- | --- |
| Bold | Ctrl/Cmd+B |
| Italic | Ctrl/Cmd+I |
| Strikethrough | Ctrl/Cmd+Shift+X |
| Inline code | Ctrl/Cmd+E |
| Link | Format > Links > Add or Edit Link… |
| Align a column left, center, or right | Format > Alignment |

To open a link, Ctrl/Cmd+click it or choose Format > Links > Open Link.

### 5.7 Undo and redo

Ctrl/Cmd+Z undoes and Ctrl/Cmd+Shift+Z redoes (on Windows, Ctrl+Y also redoes). Each document has one history. Saving or switching tables does not clear it.

## 6. Opening files

Choose File > Open… (Ctrl/Cmd+O).

### 6.1 CSV and TSV

When the delimiter and encoding are clear, Texcel opens the file directly. Otherwise, and for Shift_JIS (CP932) files, it shows a preview of the first lines with the delimiter, encoding, and header settings. Check them, then open the file. To change these settings later, close the document and open the file again.

### 6.2 XLSX and HTML

Each XLSX sheet and each HTML table is imported as a Markdown table. The original file does not become the save destination and is never changed. When you save, choose Markdown or CSV.

If some information could not be imported, Texcel reports it:

- If values, formulas, or table structure were lost, the "XLSX Import Report" or "HTML Import Report" dialog opens automatically.
- If only visual formatting such as colors or borders was lost, the status bar shows a notice with a **Details** button that opens the report.

The report has three columns: Result, Location, and Details. Select a row on the left to see its location, reason, and ranges under Details. **Save report…** writes the report to a text file.

## 7. Saving

| Command | What it does |
| --- | --- |
| Save (Ctrl/Cmd+S) | Writes to the file you opened. For an untitled or imported document it shows the save dialog |
| Save As… (Ctrl/Cmd+Shift+S) | Lets you choose the format and content, then the destination |

The save dialog has these settings:

- **Format**: Markdown or CSV.
- **Table** (CSV only): the one table to save.
- **Content**: Keep formulas or Calculated values. Keep formulas is the default.

Notes:

- Markdown with Calculated values is written as a separate export. The document's file and its unsaved mark do not change.
- A CSV file holds only the one table you chose. If other tables or Markdown formatting would be lost, Texcel lists what will be lost before saving. If CSV cannot hold the whole document, Texcel writes the CSV file but keeps the document's own file and its unsaved mark.
- Saving an unchanged Markdown or CSV file leaves every byte as it was. Text outside the tables and cells you did not edit keep their original form. Cells that Texcel writes get one space on each side of the value.
- If another app changed the file after you opened it, Texcel asks whether to overwrite it, save as a new file, or cancel.

## 8. Copy and paste

- Ctrl/Cmd+C, X, and V copy, cut, and paste cell ranges, also to and from Excel and Google Sheets.
- Edit > Copy with Options… lets you choose the format (TSV, CSV, Markdown table, or HTML) and the content (source, or displayed values).
- Edit > Paste Special… offers Values only, Transpose, and Skip Blank Cells.
- Pasting past the right or bottom edge of the table extends the table.

## 9. View settings

The View menu has these commands. None of them change the file.

- Zoom: Ctrl/Cmd++ and Ctrl/Cmd+−, and Ctrl/Cmd+0 for 100%.
- Text Wrapping: wrap long content or not.
- Freeze Panes: keep rows or columns up to the selection in view while scrolling.
- Column Width…: set a column's width as a number.

## 10. Search and data tools

- Edit > Search… (Ctrl/Cmd+F) and Replace… (Ctrl+H, Cmd+Shift+H on macOS) search the selection, the table, or the document. You can search the source or the displayed results. Replacements are shown for confirmation before they are applied.
- Data > Sort the whole table by multiple columns… sorts by several columns.
- Data > Organize trims spaces, splits or joins columns, and transposes.
- Data > Duplicates finds or removes duplicate rows.

Each of these changes can be undone in one step.

## 11. Settings

Settings… is in the Texcel menu on macOS (Cmd+,) and in the File menu on Windows and Linux. Changes apply at once and do not change your documents.

| Setting | Choices |
| --- | --- |
| App language | System, English, 日本語 |
| Theme | System, Light, Dark |
| Font | Automatic (match language), Default, Noto Sans JP |
| Zoom | Overall zoom of the interface |
| Show invisible characters | Show spaces, tabs, and line breaks as marks |

## 12. Closing, quitting, and recovered work

- When you close a document with unsaved changes, choose Save and Close, Discard Changes, or Cancel. When you quit, Texcel lists all unsaved documents together and offers Save All and Exit, Discard All and Exit, or Cancel.
- Unchanged untitled documents and documents you only imported close without a prompt.
- On macOS, closing the last window does not quit Texcel; choose Quit Texcel (Cmd+Q). On Windows and Linux, closing the last window quits Texcel.
- Texcel saves your work in progress in the background. If it stops unexpectedly, the next start reopens the unsaved work, one window per document. If you do not need it, choose Discard Changes when you close the window. Texcel never writes this work over your original files.

## 13. Keyboard shortcuts

On macOS, use Cmd instead of Ctrl, except where Control is written.

| Action | Keys |
| --- | --- |
| New document | Ctrl+N |
| Open | Ctrl+O |
| Save | Ctrl+S |
| Save As | Ctrl+Shift+S |
| Close document | Ctrl+W |
| Undo / Redo | Ctrl+Z / Ctrl+Shift+Z (Ctrl+Y also on Windows) |
| Cut / Copy / Paste | Ctrl+X / C / V |
| Select all | Ctrl+A |
| Search | Ctrl+F |
| Replace | Ctrl+H (Cmd+Shift+H on macOS) |
| Edit the cell | F2 |
| Confirm and move down / up | Enter / Shift+Enter |
| Confirm and move right / left | Tab / Shift+Tab |
| Cancel the entry | Esc |
| New line in a cell | Alt+Enter |
| Clear contents | Delete / Backspace |
| Extend the selection | Shift+arrow |
| Edge of the filled block | Ctrl+arrow (add Shift to select) |
| One screen up / down | Page Up / Page Down |
| One screen left / right (Windows) | Alt+Page Up / Page Down |
| Start / end of the row | Home / End |
| Start of the table / end of the used range | Ctrl+Home / End |
| Select whole column / row | Control+Space / Shift+Space |
| Copy from above / left | Ctrl+D / R |
| Change the reference type | F4 (Cmd+T on macOS) |
| Bold / Italic | Ctrl+B / I |
| Strikethrough | Ctrl+Shift+X |
| Inline code | Ctrl+E |
| Live Editing / Source Editing | Ctrl+/ |
| Add / duplicate a table | Ctrl+Shift+N / D |
| Zoom in / out / 100% | Ctrl++ / − / 0 |
| Settings (macOS) | Cmd+, |
| Minimize (macOS) | Cmd+M |

## 14. Where settings and recovered work are stored

| OS | Folder |
| --- | --- |
| macOS | `~/Library/Application Support/Texcel/` |
| Windows | `%APPDATA%\Texcel\` |
| Linux | `$XDG_CONFIG_HOME/Texcel/` (`~/.config/Texcel/` if unset) |

`settings.json` holds the settings. The `recovery` folder holds work saved in case Texcel stops unexpectedly.

## 15. Uninstalling

- macOS: move `Texcel.app` to the Trash.
- Windows: delete the extracted folder.
- Linux: delete the copied `bin/texcel`, `share/applications/net.homeunix.hio.app.Texcel.desktop`, `share/icons/hicolor/256x256/apps/texcel.png`, and `share/doc/texcel`.

To remove the settings too, delete the folder listed in section 14.

## 16. Troubleshooting

| Problem | What to do |
| --- | --- |
| macOS says it cannot open Texcel | Follow section 3.1 and select Open Anyway |
| Windows shows "Windows protected your PC" | Select More info, then Run anyway |
| A GPU error appears at startup | Update the GPU driver. Remote desktop sessions and virtual machines may not support drawing |
| The file dialog does not open on Linux | Install and run an XDG desktop portal service |
| CSV text looks garbled | Close the document, open the file again, and choose the encoding in the preview |
| Numbers are not added up | They are text that Texcel does not treat as numbers, such as values with leading zeros. Convert them with `=VALUE()` |
| You are not sure how to do something | Open Help > Texcel Help… |
