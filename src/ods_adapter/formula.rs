//!  Rewrites OpenFormula text into the A1 syntax the calculation engine parses.
//!
//!  This is a lexical rewrite only: string literals are copied, bracketed references and
//!  separators are translated, and the result is parsed later by the engine itself.

/// Converts an ODS `table:formula` value into a formula without its leading `=`.
/// Returns `None` for namespaces, references, or operators that have no A1 equivalent.
pub(crate) fn convert_open_formula(raw: &str) -> Option<String> {
    let (namespace, body) = match raw.split_once(':') {
        Some((namespace, body)) if !namespace.contains(['=', '"', '[']) => (namespace, body),
        _ => ("of", raw),
    };
    let body = body.strip_prefix('=')?;
    match namespace {
        // Excel writes its own syntax into ODS files under this namespace.
        "msoxl" => Some(body.to_owned()),
        "of" | "oooc" => rewrite_body(body),
        _ => None,
    }
}

/// Walks the formula once, translating everything outside string literals.
fn rewrite_body(body: &str) -> Option<String> {
    let characters = body.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(body.len());
    let mut index = 0;
    let mut array_depth = 0_usize;
    while index < characters.len() {
        let character = characters[index];
        match character {
            '"' => {
                // Copy the literal, including doubled quotes, unchanged.
                output.push('"');
                index += 1;
                loop {
                    let next = *characters.get(index)?;
                    output.push(next);
                    index += 1;
                    if next == '"' {
                        if characters.get(index) == Some(&'"') {
                            output.push('"');
                            index += 1;
                        } else {
                            break;
                        }
                    }
                }
                continue;
            }
            '[' => {
                let (reference, end) = bracketed(&characters, index + 1)?;
                output.push_str(&convert_reference(&reference)?);
                index = end + 1;
                continue;
            }
            '{' => {
                array_depth += 1;
                output.push('{');
            }
            '}' => {
                array_depth = array_depth.checked_sub(1)?;
                output.push('}');
            }
            // Error constants such as `#DIV/0!` pass through, including their `!`.
            '#' => {
                output.push('#');
                index += 1;
                while let Some(next) = characters.get(index).copied() {
                    if !(next.is_ascii_alphanumeric() || matches!(next, '/' | '!' | '?' | '_')) {
                        break;
                    }
                    output.push(next);
                    index += 1;
                }
                continue;
            }
            // Inline arrays separate columns with `;` and rows with `|`.
            ';' => output.push(','),
            '|' if array_depth > 0 => output.push(';'),
            // Reference union and intersection have no equivalent in the engine's syntax.
            '~' | '!' => return None,
            _ if character.is_ascii_alphabetic() || character == '_' => {
                let start = index;
                while index < characters.len()
                    && (characters[index].is_ascii_alphanumeric()
                        || matches!(characters[index], '.' | '_'))
                {
                    index += 1;
                }
                let name = characters[start..index].iter().collect::<String>();
                // LibreOffice marks Excel-only functions with this prefix.
                let name = name
                    .strip_prefix("COM.MICROSOFT.")
                    .map_or(name.as_str(), |rest| rest);
                output.push_str(name);
                continue;
            }
            _ => output.push(character),
        }
        index += 1;
    }
    (array_depth == 0).then_some(output)
}

/// Returns the text inside `[...]` starting at `start`, honoring quoted sheet names.
fn bracketed(characters: &[char], start: usize) -> Option<(String, usize)> {
    let mut quoted = false;
    let mut index = start;
    while index < characters.len() {
        match characters[index] {
            '\'' if quoted && characters.get(index + 1) == Some(&'\'') => index += 1,
            '\'' => quoted = !quoted,
            ']' if !quoted => {
                return Some((characters[start..index].iter().collect(), index));
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// Converts `.A1`, `$Sheet.A1:.B2`, or `$'My Sheet'.A1` into `A1`, `'Sheet'!A1:B2`, and so on.
fn convert_reference(reference: &str) -> Option<String> {
    let reference = reference.trim();
    if reference.contains("#REF!") {
        return Some("#REF!".to_owned());
    }
    let parts = split_outside_quotes(reference, ':');
    let (first_sheet, first_cell) = split_sheet(parts.first()?)?;
    let mut converted = String::new();
    if let Some(sheet) = &first_sheet {
        converted.push_str(&quote_sheet(sheet));
        converted.push('!');
    }
    converted.push_str(&first_cell);
    match parts.as_slice() {
        [_] => {}
        [_, second] => {
            let (second_sheet, second_cell) = split_sheet(second)?;
            // A range across sheets has no A1 form.
            if second_sheet.is_some() && second_sheet != first_sheet {
                return None;
            }
            converted.push(':');
            converted.push_str(&second_cell);
        }
        _ => return None,
    }
    Some(converted)
}

/// Splits `$Sheet.A1` into its optional sheet name and checked cell part.
fn split_sheet(part: &str) -> Option<(Option<String>, String)> {
    // External workbooks are written as `'file:...'#$Sheet.A1`.
    if split_outside_quotes(part, '#').len() > 1 {
        return None;
    }
    let pieces = split_outside_quotes(part, '.');
    let (sheet, cell) = match pieces.as_slice() {
        [cell] => (None, cell.as_str()),
        [sheet, cell] => (Some(sheet.as_str()), cell.as_str()),
        _ => return None,
    };
    let sheet = match sheet
        .map(|sheet| sheet.strip_prefix('$').unwrap_or(sheet))
        .filter(|sheet| !sheet.is_empty())
    {
        Some(sheet) => Some(unquote_sheet(sheet)?),
        None => None,
    };
    is_cell_part(cell).then(|| (sheet, cell.to_owned()))
}

/// Accepts `A1`, `$A$1`, a whole column `A`, or a whole row `1`, with optional `$` marks.
fn is_cell_part(cell: &str) -> bool {
    let bytes = cell.as_bytes();
    let mut index = 0;
    let skip_dollar = |index: &mut usize| {
        if bytes.get(*index) == Some(&b'$') {
            *index += 1;
        }
    };
    skip_dollar(&mut index);
    let letters = bytes[index..]
        .iter()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .count();
    index += letters;
    if letters > 0 {
        skip_dollar(&mut index);
    }
    let digits = bytes[index..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    index += digits;
    index == bytes.len() && letters <= 3 && digits <= 7 && letters + digits > 0
}

/// Removes ODF quoting from a sheet name, undoing doubled quotes.
fn unquote_sheet(sheet: &str) -> Option<String> {
    match sheet.strip_prefix('\'') {
        Some(rest) => Some(rest.strip_suffix('\'')?.replace("''", "'")),
        None => Some(sheet.to_owned()),
    }
}

/// Quotes a sheet name for the engine's `'Sheet'!A1` syntax.
fn quote_sheet(sheet: &str) -> String {
    format!("'{}'", sheet.replace('\'', "''"))
}

/// Splits on `separator` except inside single-quoted names.
fn split_outside_quotes(text: &str, separator: char) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut quoted = false;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\'' if quoted && characters.peek() == Some(&'\'') => {
                characters.next();
                if let Some(last) = parts.last_mut() {
                    last.push_str("''");
                }
                continue;
            }
            '\'' => quoted = !quoted,
            _ if character == separator && !quoted => {
                parts.push(String::new());
                continue;
            }
            _ => {}
        }
        if let Some(last) = parts.last_mut() {
            last.push(character);
        }
    }
    parts
}
