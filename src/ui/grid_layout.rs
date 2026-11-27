use super::*;

#[derive(Default)]
pub(super) struct GridRowLayout {
    pub(super) row_count: usize,
    pub(super) layout_revision: u64,
    pub(super) display_generation: u64,
    /// Live/source mode and invisible marks the heights were measured with.
    pub(super) text_display: Option<(CellDisplayMode, bool)>,
    pub(super) measured_rows: HashMap<usize, (u64, u64, f32)>,
    pub(super) height_deltas: HashMap<usize, f32>,
    /// Grid scale the stored heights were measured at; zero means 100%.
    pub(super) scale: f32,
}

impl GridRowLayout {
    /// Resets cached row measurements when the table layout changes.
    pub(super) fn sync_layout_revision(&mut self, layout_revision: u64) {
        if self.layout_revision == layout_revision {
            return;
        }
        self.layout_revision = layout_revision;
        self.measured_rows.clear();
        self.height_deltas.clear();
    }

    /// Resets cached row measurements when display fonts or settings change.
    pub(super) fn sync_display_generation(&mut self, generation: u64) {
        if self.display_generation == generation {
            return;
        }
        self.display_generation = generation;
        self.measured_rows.clear();
        self.height_deltas.clear();
    }

    /// Resets cached row measurements when live/source mode or invisible marks change.
    /// Heights only grow within one document revision, so a switch must start over.
    pub(super) fn sync_text_display(&mut self, mode: CellDisplayMode, show_invisibles: bool) {
        let key = Some((mode, show_invisibles));
        if self.text_display == key {
            return;
        }
        self.text_display = key;
        self.measured_rows.clear();
        self.height_deltas.clear();
    }

    /// Resets cached row measurements when the sheet zoom changes.
    pub(super) fn sync_scale(&mut self, scale: f32) {
        if self.scale == scale {
            return;
        }
        self.scale = scale;
        self.measured_rows.clear();
        self.height_deltas.clear();
    }

    /// Returns the zoomed height of a row with no measured content.
    pub(super) fn base_height(&self) -> f32 {
        ROW_HEIGHT * self.scale_or_one()
    }

    /// Returns the zoomed upper limit for one wrapped row.
    pub(super) fn max_height(&self) -> f32 {
        MAX_WRAPPED_ROW_HEIGHT * self.scale_or_one()
    }

    /// Treats a layout that was never scaled as 100%.
    fn scale_or_one(&self) -> f32 {
        if self.scale > 0.0 { self.scale } else { 1.0 }
    }

    /// Keeps the row-height tree sized for the current table.
    pub(super) fn ensure_row_count(&mut self, row_count: usize) {
        if self.row_count == row_count {
            return;
        }
        self.row_count = row_count;
        self.measured_rows.retain(|row, _| *row < row_count);
        self.height_deltas.clear();
        let measured = self
            .measured_rows
            .iter()
            .map(|(row, (_, _, height))| (*row, *height))
            .collect::<Vec<_>>();
        for (row, height) in measured {
            self.add_height_delta(row, height - self.base_height());
        }
    }

    /// Returns the measured or default height for one row.
    pub(super) fn row_height(&self, row: usize) -> f32 {
        self.measured_rows
            .get(&row)
            .map_or(self.base_height(), |(_, _, height)| *height)
    }

    /// Returns the total height of the rows before a row index.
    pub(super) fn prefix_height(&self, rows: usize) -> f32 {
        rows.min(MAX_ROWS) as f32 * self.base_height() + self.prefix_delta(rows)
    }

    /// Returns accumulated measured-height changes before an index.
    pub(super) fn prefix_delta(&self, rows: usize) -> f32 {
        let mut index = rows.min(self.row_count);
        let mut sum = 0.0;
        while index > 0 {
            sum += self.height_deltas.get(&index).copied().unwrap_or_default();
            index &= index - 1;
        }
        sum
    }

    /// Finds the row at a vertical position in the current layout.
    pub(super) fn row_at_position(&self, position: f32) -> usize {
        if self.row_count == 0 {
            return 0;
        }
        let target = position.max(0.0);
        let mut low = 0;
        let mut high = self.row_count;
        while low < high {
            let middle = low + (high - low) / 2;
            if self.prefix_height(middle + 1) <= target {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low.min(self.row_count - 1)
    }

    /// Updates one measured row and returns its height change for scroll anchoring.
    pub(super) fn update_row(
        &mut self,
        row: usize,
        document_revision: u64,
        fingerprint: u64,
        height: f32,
    ) -> f32 {
        self.update_row_with_base(
            row,
            document_revision,
            fingerprint,
            height,
            self.base_height(),
        )
    }

    /// Stores a measured row height using the current base height.
    pub(super) fn update_row_with_base(
        &mut self,
        row: usize,
        document_revision: u64,
        fingerprint: u64,
        mut height: f32,
        base_height: f32,
    ) -> f32 {
        height = height.clamp(base_height, self.max_height().max(base_height));
        if let Some((known_revision, known_fingerprint, known_height)) =
            self.measured_rows.get(&row)
        {
            if *known_revision == document_revision && *known_fingerprint == fingerprint {
                return 0.0;
            }
            if *known_revision == document_revision {
                height = height.max(*known_height);
            }
        }
        let old_height = self.row_height(row);
        self.measured_rows
            .insert(row, (document_revision, fingerprint, height));
        let delta = height - old_height;
        if delta != 0.0 {
            self.add_height_delta(row, delta);
        }
        delta
    }

    /// Returns a cached row height when its inputs still match.
    pub(super) fn measured_height(
        &self,
        row: usize,
        document_revision: u64,
        fingerprint: u64,
    ) -> Option<f32> {
        self.measured_rows
            .get(&row)
            .filter(|(known_revision, _, _)| *known_revision == document_revision)
            .filter(|(_, known_fingerprint, _)| *known_fingerprint == fingerprint)
            .map(|(_, _, height)| *height)
    }

    /// Updates the prefix tree for one measured height change.
    pub(super) fn add_height_delta(&mut self, row: usize, delta: f32) {
        let mut index = row + 1;
        while index <= self.row_count {
            let value = self.height_deltas.get(&index).copied().unwrap_or_default() + delta;
            if value.abs() < 0.01 {
                self.height_deltas.remove(&index);
            } else {
                self.height_deltas.insert(index, value);
            }
            index += index.isolate_lowest_one();
        }
    }
}

/// Collects text and style inputs used to measure wrapped rows.
pub(super) fn wrapped_row_cells_for_ranges(
    document: &Document,
    table: &crate::document::Table,
    layout: ScaledLayout<'_>,
    row: usize,
    visible_column_ranges: &[std::ops::Range<usize>],
    wrap_text: bool,
) -> (u64, Vec<(CellAddress, String, f32)>) {
    let mut hasher = DefaultHasher::new();
    row.hash(&mut hasher);
    let mut wrapped_cells = Vec::new();
    for visible_columns in visible_column_ranges {
        visible_columns.start.hash(&mut hasher);
        visible_columns.end.hash(&mut hasher);
        for (address, cell) in table.stored_cells_in_row_columns(row, visible_columns.clone()) {
            if !wrap_text || layout.column_width_pixels(address.column) <= 0.0 {
                continue;
            }
            address.column.hash(&mut hasher);
            let width = layout.column_width_pixels(address.column) as f32;
            width.to_bits().hash(&mut hasher);
            let calculation = document.calculation_result_for(table.id, address);
            let text = cell.display_text_with_calculation(calculation.as_ref());
            text.hash(&mut hasher);
            wrapped_cells.push((address, text, width));
        }
    }
    (hasher.finish(), wrapped_cells)
}

/// Finds the column at a horizontal position.
pub(super) fn column_at_position(layout: ScaledLayout<'_>, columns: usize, position: f32) -> usize {
    if columns == 0 {
        return 0;
    }
    let target = f64::from(position.max(0.0));
    let mut low = 0;
    let mut high = columns;
    while low < high {
        let middle = low + (high - low) / 2;
        if layout.prefix_column_width_pixels(middle + 1) <= target {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low.min(columns - 1)
}

pub(super) struct GridPointerGeometry<'a> {
    pub(super) viewport: Rect,
    pub(super) body_min: Pos2,
    pub(super) scroll_x: f32,
    pub(super) scroll_y: f32,
    pub(super) frozen_width: f32,
    pub(super) frozen_height: f32,
    pub(super) layout: ScaledLayout<'a>,
    pub(super) row_layout: &'a GridRowLayout,
    pub(super) rows: usize,
    pub(super) columns: usize,
}
