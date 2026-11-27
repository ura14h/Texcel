# Texcel Help

Texcel edits Markdown tables like a spreadsheet. It also edits CSV and TSV files, and imports XLSX, ODS, and HTML as Markdown tables.

On macOS, read Ctrl as Cmd below, except where Control is written out.

## The window

| Area | What it does |
| --- | --- |
| Name box | The selected cell's address. Type `C12` or `A1:D20` and press Enter to select it |
| Buttons left of the formula bar | Cancel the entry, confirm it, or insert a function (fx) |
| Formula bar | The selected cell's source, with formulas and Markdown marks. Click it to edit from that point |
| Live / Source | Right of the formula bar. Switches the display mode |
| Table tabs and List | Switch between the tables in the document. **+** adds a table |
| Zoom | The sheet zoom. Click it to choose another zoom |
| Status bar | Finished work, warnings, and how many cells, rows, and columns are selected |

**Live Editing** shows formula results and Markdown formatting. **Source Editing** shows every cell's source. Ctrl+/ also switches; the document does not change.

## Selecting and moving

- Click or use the arrow keys to select a cell. Shift+arrows, dragging, or Shift+click select a range; Ctrl+click adds a separate range.
- Click a column letter or row number to select the whole column or row. Drag across the headers or Shift+click to select adjacent columns or rows; Ctrl+click adds ones that are apart.
- Click the top-left corner or press Ctrl+A to select every cell.
- Ctrl+arrow moves to the edge of the data; Ctrl+Home and Ctrl+End go to the start of the table and the end of the used range.

## Entering and editing

- Select a cell and type to replace its contents. Japanese input starts the same way.
- To change the current contents, press F2 or double-click.
- Enter confirms and moves down; Tab confirms and moves right. Esc cancels the entry.
- Alt+Enter starts a new line in the cell. In a Markdown document it types `<br>`.
- Delete or Backspace clears the selected cells.

## Formulas

Content that starts with `=` is a formula, such as `=B2*C2` or `=SUM(D2:D3)`.

- While typing a formula, click or drag cells to insert their addresses.
- F4 (Cmd+T on macOS) cycles a reference through relative, absolute, row-fixed, and column-fixed.
- **fx** opens the list of functions. Type part of a name to filter it.
- To enter text that starts with `=`, put `'` in front of it.
- Text with leading zeros, such as `00123`, stays text. Totals such as SUM ignore it.

Functions include arithmetic, comparison, and text joining, and SUM, AVERAGE, MIN, MAX, COUNT, COUNTA, COUNTIF, SUMIF, IF, IFERROR, AND, OR, NOT, ABS, ROUND, ROUNDUP, ROUNDDOWN, LEN, LEFT, RIGHT, MID, TRIM, CONCATENATE, VALUE, DATE, YEAR, MONTH, and DAY.

## Fill

Drag the small square at the lower right of the selection to fill that range. Evenly spaced numbers, dates, and trailing numbers continue as a series; anything else is copied. Ctrl+D and Ctrl+R copy the cell above or to the left.

## Rows, columns, and tables

- Insert rows and columns from the Insert menu (Ctrl+Shift+R and C). Delete, duplicate, and move them from Table > Rows and Table > Columns, or from the right-click menus.
- Add, duplicate, and delete tables from the Table menu or a table tab's right-click menu.
- Drag the border between column letters to change a width, or double-click it to fit the contents. View > Column Width… sets it in half-width characters.

## Markdown formatting

| Format | How |
| --- | --- |
| Bold, italic | Ctrl+B, Ctrl+I |
| Strikethrough | Ctrl+Shift+X |
| Inline code | Ctrl+E |
| Link | Format > Links |
| Left, center, or right column alignment | Format > Alignment |

Formatting applies to every selected range at once. Ctrl+click opens a link.

## Undo and redo

Ctrl+Z undoes and Ctrl+Shift+Z redoes (Ctrl+Y also redoes on Windows). Each document has its own history, which saving keeps.

## Opening and saving

- Open files with Open… (Ctrl+O), by dropping them on a window, or from Open Recent. XLSX, ODS, and HTML are imported as Markdown tables; the original file is not changed.
- Save (Ctrl+S) writes to the file you opened. Save As… (Ctrl+Shift+S) lets you choose the format (Markdown, CSV, or TSV) and whether to keep formulas or write calculated values.
- CSV and TSV hold only the one table you choose. Anything that would be lost is listed before saving.

## Copy and paste

- Ctrl+C, X, and V copy, cut, and paste, also with Excel and Google Sheets.
- Ctrl+Shift+V pastes as text.
- Edit > Copy with Options… and Paste Special… (Ctrl+Alt+V, or Cmd+Control+V on macOS) let you choose the format and content.

## View

- Ctrl++ and Ctrl+− change the sheet zoom; Ctrl+0 returns to 100%.
- The View menu turns text wrapping on or off, freezes panes, and fits column widths.
- Settings… changes the language, theme, font, interface text size, and invisible marks.

## Search and data tools

- Edit > Search… (Ctrl+F) and Replace… (Ctrl+H, or Cmd+Shift+H on macOS) search and replace.
- The Data menu sorts, trims spaces, splits and joins columns, transposes, and finds or removes duplicate rows. Each of these can be undone in one step.
