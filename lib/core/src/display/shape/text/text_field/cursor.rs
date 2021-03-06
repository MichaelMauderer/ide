//! Module with structured describing cursors and their selectino in a TextField.

use crate::prelude::*;

use crate::display::shape::text::text_field::content::line::LineFullInfo;
use crate::display::shape::text::text_field::content::TextFieldContent;

use data::text::TextLocation;
use nalgebra::Vector2;
use nalgebra::min;
use std::cmp::Ordering;
use std::ops::Range;


// ==============
// === Cursor ===
// ==============

/// Cursor in TextComponent with its selection.
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub struct Cursor {
    /// Cursor's position in text.
    pub position: TextLocation,
    /// A position when the selection of cursor ends. It may be before or after the cursor position.
    pub selected_to: TextLocation,
}

impl Cursor {
    /// Create a new cursor at given position and without any selection.
    pub fn new(position:TextLocation) -> Self {
        let selected_to = position;
        Cursor {position,selected_to}
    }

    /// Recalculate cursor position adjusting itself to new content.
    pub fn recalculate_position(&mut self, content:&TextFieldContent) {
        let lines               = content.lines();
        let max_line_index      = lines.len() - 1;
        self.position.line      = min(self.position.line,max_line_index);
        self.selected_to.line   = min(self.selected_to.line,max_line_index);
        let max_column_index    = lines[self.position.line].len();
        self.position.column    = min(self.position.column,max_column_index);
        let max_column_index    = lines[self.selected_to.line].len();
        self.selected_to.column = min(self.selected_to.column,max_column_index);
    }

    /// Returns true if some selection is bound to this cursor.
    pub fn has_selection(&self) -> bool {
        self.position != self.selected_to
    }

    /// Select text range.
    pub fn select_range(&mut self, range:&Range<TextLocation>) {
        self.position    = range.end;
        self.selected_to = range.start;
    }

    /// Get range of selected text by this cursor.
    pub fn selection_range(&self) -> Range<TextLocation> {
        match self.position.cmp(&self.selected_to) {
            Ordering::Equal   => self.position..self.position,
            Ordering::Greater => self.selected_to..self.position,
            Ordering::Less    => self.position..self.selected_to
        }
    }

    /// Extend the selection to cover the given range. Cursor itself may be moved, and will be
    /// on the same side of selection as before.
    pub fn extend_selection(&mut self, range:&Range<TextLocation>) {
        let new_start = range.start.min(self.position).min(self.selected_to);
        let new_end   = range.end.max(self.position).max(self.selected_to);
        *self = match self.position.cmp(&self.selected_to) {
            Ordering::Less => Cursor{position:new_start, selected_to:new_end  },
            _              => Cursor{position:new_end  , selected_to:new_start},
        }
    }

    /// Check if char at given position is selected.
    pub fn is_char_selected(&self, position:TextLocation) -> bool {
        self.selection_range().contains(&position)
    }

    /// Get `LineFullInfo` object of this cursor's line.
    pub fn current_line<'a>(&self, content:&'a mut TextFieldContent)
    -> LineFullInfo<'a> {
        content.line(self.position.line)
    }

    /// Get the position where the cursor should be rendered. The returned point is on the
    /// middle of line's height, on the right side of character from the left side of the cursor
    /// (where usually the cursor is displayed by text editors).
    ///
    /// _Baseline_ is a font specific term, for details see [freetype documentation]
    ///  (https://www.freetype.org/freetype2/docs/glyphs/glyphs-3.html#section-1).
    pub fn render_position(position:&TextLocation, content:&mut TextFieldContent) -> Vector2<f32> {
        let line_height = content.line_height;
        let mut line    = content.line(position.line);
        // TODO[ao] this value should be read from font information, but msdf_sys library does
        // not provide it yet.
        let descender = line.baseline_start().y - 0.15 * line_height;
        let x         = Self::x_position_of_cursor_at(position.column,&mut line);
        let y         = descender + line_height / 2.0;
        Vector2::new(x,y)
    }

    fn x_position_of_cursor_at(column:usize, line:&mut LineFullInfo) -> f32 {
        if column > 0 {
            let char_index = column - 1;
            line.get_char_x_range(char_index).end
        } else {
            line.baseline_start().x
        }
    }
}



// ==================
// === Navigation ===
// ==================

/// An enum representing cursor moving step. The steps are based of possible keystrokes (arrows,
/// Home, End, Ctrl+Home, etc.)
#[derive(Copy,Clone,Debug,Eq,Hash,PartialEq)]
#[allow(missing_docs)]
pub enum Step {Left,Right,Up,Down,LineBegin,LineEnd,DocBegin,DocEnd}

/// A struct for cursor navigation process.
#[derive(Debug)]
pub struct CursorNavigation<'a> {
    /// A reference to text content. This is required to obtain the x positions of chars for proper
    /// moving cursors up and down.
    pub content: &'a mut TextFieldContent,
    /// Selecting navigation selects/unselects all text between current and new cursor position.
    pub selecting: bool
}

impl<'a> CursorNavigation<'a> {
    /// Jump cursor directly to given position.
    pub fn move_cursor_to_position(&self, cursor:&mut Cursor, to:TextLocation) {
        cursor.position = to;
        if !self.selecting {
            cursor.selected_to = to;
        }
    }

    /// Jump cursor to the nearest position from given point on the screen.
    pub fn move_cursor_to_point(&mut self, cursor:&mut Cursor, to:Vector2<f32>) {
        let position = self.content.location_at_point(to);
        self.move_cursor_to_position(cursor,position);
    }

    /// Move cursor by given step.
    pub fn move_cursor(&mut self, cursor:&mut Cursor, step:Step) {
        let new_position = self.new_position(cursor.position,step);
        self.move_cursor_to_position(cursor,new_position);
    }

    /// Get cursor position at end of given line.
    pub fn line_end_position(&self, line_index:usize) -> TextLocation {
        TextLocation {
            line   : line_index,
            column : self.content.lines()[line_index].len(),
        }
    }

    /// Get cursor position at end of whole content
    pub fn content_end_position(&self) -> TextLocation {
        TextLocation {
            column : self.content.lines().last().unwrap().len(),
            line   : self.content.lines().len() - 1,
        }
    }

    /// Get cursor position for the next char from given position. Returns none if at end of
    /// whole document.
    pub fn next_char_position(&self, position:&TextLocation) -> Option<TextLocation> {
        let current_line = &self.content.lines()[position.line];
        let next_column  = Some(position.column + 1).filter(|c| *c <= current_line.len());
        let next_line    = Some(position.line + 1)  .filter(|l| *l < self.content.lines().len());
        match (next_column,next_line) {
            (None         , None      ) => None,
            (None         , Some(line)) => Some(TextLocation::at_line_begin(line)),
            (Some(column) , _         ) => Some(TextLocation {column, ..*position})
        }
    }

    /// Get cursor position for the previous char from given position. Returns none if at begin of
    /// whole document.
    pub fn prev_char_position(&self, position:&TextLocation) -> Option<TextLocation> {
        let prev_column = position.column.checked_sub(1);
        let prev_line   = position.line.checked_sub(1);
        match (prev_column,prev_line) {
            (None         , None      ) => None,
            (None         , Some(line)) => Some(self.line_end_position(line)),
            (Some(column) , _         ) => Some(TextLocation {column, ..*position})
        }
    }

    /// Get cursor position one line above the given position, such the new x coordinate of
    /// displayed cursor on the screen will be nearest the current value.
    pub fn line_up_position(&mut self, position:&TextLocation) -> Option<TextLocation> {
        let prev_line = position.line.checked_sub(1);
        prev_line.map(|line| self.near_same_x_in_another_line(position,line))
    }

    /// Get cursor position one line behind the given position, such the new x coordinate of
    /// displayed cursor on the screen will be nearest the current value.
    pub fn line_down_position(&mut self, position:&TextLocation) -> Option<TextLocation> {
        let next_line = Some(position.line + 1).filter(|l| *l < self.content.lines().len());
        next_line.map(|line| self.near_same_x_in_another_line(position,line))
    }

    /// New position of cursor at `position` after applying `step`.
    fn new_position(&mut self, position: TextLocation, step:Step) -> TextLocation {
        match step {
            Step::Left      => self.prev_char_position(&position).unwrap_or(position),
            Step::Right     => self.next_char_position(&position).unwrap_or(position),
            Step::Up        => self.line_up_position(&position).unwrap_or(position),
            Step::Down      => self.line_down_position(&position).unwrap_or(position),
            Step::LineBegin => TextLocation::at_line_begin(position.line),
            Step::LineEnd   => self.line_end_position(position.line),
            Step::DocBegin  => TextLocation::at_document_begin(),
            Step::DocEnd    => self.content_end_position(),
        }
    }

    /// Get the cursor position on another line, such that the new x coordinate of
    /// displayed cursor on the screen will be nearest the current value.
    fn near_same_x_in_another_line(&mut self, position:&TextLocation, line_index:usize)
    -> TextLocation {
        let mut line   = self.content.line(position.line);
        let x_position = Cursor::x_position_of_cursor_at(position.column,&mut line);
        let column     = self.column_near_x(line_index,x_position);
        TextLocation {line:line_index, column}
    }

    /// Get the column number in given line, so the cursor will be as near as possible the
    /// `x_position` in _text space_. See `display::shape::text::content::line::Line`
    /// documentation for details about _text space_.
    fn column_near_x(&mut self, line_index:usize, x_position:f32) -> usize {
        let mut line                = self.content.line(line_index);
        let x                       = x_position;
        let char_at_x               = line.find_char_at_x_position(x);
        let nearer_to_end           = |range:Range<f32>| range.end - x < x - range.start;
        let mut nearer_to_chars_end = |index| nearer_to_end(line.get_char_x_range(index));
        match char_at_x {
            Some(index) if nearer_to_chars_end(index) => index + 1,
            Some(index)                               => index,
            None                                      => line.len()
        }
    }
}



// ===============
// === Cursors ===
// ===============



/// A newtype for cursor id.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,PartialOrd,Ord)]
pub struct CursorId(pub usize);

/// Structure handling many cursors.
///
/// Usually there is only one cursor, but we have possibility of having many cursors in one text
/// component enabling editing in multiple lines/places at once.
#[derive(Debug)]
pub struct Cursors {
    /// All cursors' positions.
    pub cursors : Vec<Cursor>,
}

impl Default for Cursors {
    fn default() -> Self {
        Cursors {
            cursors : vec![Cursor::new(TextLocation::at_document_begin())],
        }
    }
}

impl Cursors {
    /// Removes all current cursors and replace them with single cursor without any selection.
    pub fn set_cursor(&mut self, position: TextLocation) {
        self.cursors = vec![Cursor::new(position)];
    }

    /// Recalculate cursors positions adjusting to new content.
    pub fn recalculate_positions(&mut self, content:&TextFieldContent) {
        for cursor in &mut self.cursors {
            cursor.recalculate_position(content);
        }
        self.merge_overlapping_cursors();
    }

    /// Remove all cursors except the active one.
    pub fn remove_additional_cursors(&mut self) {
        self.cursors.drain(0..self.cursors.len()-1);
    }

    /// Return the active (last added) cursor as mutable reference. Even on multiline edit some
    /// operations are applied to one, active cursor only (e.g. extending selection by mouse).
    pub fn active_cursor_mut(&mut self) -> &mut Cursor {
        self.cursors.last_mut().unwrap()
    }

    /// Return the active (last added) cursor. Even on multiline edit some operations are applied
    /// to one, active cursor only (e.g. extending selection by mouse).
    pub fn active_cursor(&self) -> &Cursor {
        self.cursors.last().unwrap()
    }

    /// Add new cursor without selection.
    pub fn add_cursor(&mut self, position: TextLocation) {
        self.cursors.push(Cursor::new(position));
        self.merge_overlapping_cursors();
    }

    /// Do the navigation step of all cursors.
    ///
    /// If after this operation some of the cursors occupies the same position, or their selected
    /// area overlap, they are irreversibly merged.
    pub fn navigate_all_cursors(&mut self, navigation:&mut CursorNavigation, step:Step) {
        self.navigate_cursors(navigation,step,|_| true)
    }

    /// Do the navigation step of all cursors satisfying given predicate.
    ///
    /// If after this operation some of the cursors occupies the same position, or their selected
    /// area overlap, they are irreversibly merged.
    pub fn navigate_cursors<Predicate>
    (&mut self, navigation:&mut CursorNavigation, step:Step, mut predicate:Predicate)
    where Predicate : FnMut(&Cursor) -> bool {
        let filtered = self.cursors.iter_mut().filter(|c| predicate(c));
        filtered.for_each(|cursor| navigation.move_cursor(cursor, step));
        self.merge_overlapping_cursors();
    }

    /// Jump the active (last) cursor to the nearest location from given point of the screen.
    ///
    /// If after this operation some of the cursors occupies the same position, or their selected
    /// area overlap, they are irreversibly merged.
    pub fn jump_cursor(&mut self, navigation:&mut CursorNavigation, point:Vector2<f32>) {
        navigation.move_cursor_to_point(self.cursors.last_mut().unwrap(),point);
        self.merge_overlapping_cursors();
    }

    /// Returns cursor indices sorted by cursors' position in text.
    pub fn sorted_cursor_indices(&self) -> Vec<CursorId> {
        let sorted_pairs = self.cursors.iter().enumerate().sorted_by_key(|(_,c)| c.position);
        sorted_pairs.map(|(i,_)| CursorId(i)).collect()
    }

    /// Merge overlapping cursors
    ///
    /// This function checks all cursors, and merge each pair where cursors are at the same position
    /// or their selection overlap.
    ///
    /// The merged pair will be replaced with one cursor with selection being a sum of selections of
    /// removed cursors.
    fn merge_overlapping_cursors(&mut self) {
        if !self.cursors.is_empty() {
            let sorted             = self.sorted_cursor_indices();
            let mut to_remove      = Vec::new();
            let mut last_cursor_id = sorted[0];
            for id in sorted.iter().skip(1) {
                let merged = self.merged_selection_range(last_cursor_id,*id);
                match merged {
                    Some(merged_range) => {
                        self.cursors[last_cursor_id.0].extend_selection(&merged_range);
                        to_remove.push(*id);
                    },
                    None => {
                        last_cursor_id = *id;
                    }
                };
            }
            for id in to_remove.iter().sorted().rev() {
                self.cursors.remove(id.0);
            }
        }
    }

    /// Checks if two cursors should be merged and returns new selection range after merging if they
    /// shoukd, and `None` otherwise.
    fn merged_selection_range(&self, left_cursor_index:CursorId, right_cursor_index:CursorId)
    -> Option<Range<TextLocation>> {
        let CursorId(left_id)           = left_cursor_index;
        let CursorId(right_id)          = right_cursor_index;
        let left_cursor_position        = self.cursors[left_id].position;
        let left_cursor_range           = self.cursors[left_id].selection_range();
        let right_cursor_position       = self.cursors[right_id].position;
        let right_cursor_range          = self.cursors[right_id].selection_range();
        let are_cursor_at_same_position = left_cursor_position == right_cursor_position;
        let are_ranges_overlapping      = right_cursor_range.start < left_cursor_range.end;
        let are_cursors_merged          = are_cursor_at_same_position || are_ranges_overlapping;
        are_cursors_merged.and_option_from(|| {
            let new_start = left_cursor_range.start.min(right_cursor_range.start);
            let new_end   = left_cursor_range.end  .max(right_cursor_range.end  );
            Some(new_start..new_end)
        })
    }

    #[cfg(test)]
    fn mock(cursors:Vec<Cursor>) -> Self {
        Cursors{cursors}
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use Step::*;

    use basegl_core_msdf_sys as msdf_sys;

    use crate::display::shape::text::glyph::font::FontRegistry;
    use crate::display::shape::text::text_field::content::TextFieldContent;
    use crate::display::shape::text::text_field::content::test::mock_properties;
    use crate::display::shape::text::text_field::TextFieldProperties;

    use wasm_bindgen_test::wasm_bindgen_test;
    use wasm_bindgen_test::wasm_bindgen_test_configure;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test(async)]
    async fn moving_cursors() {
        basegl_core_msdf_sys::initialized().await;
        let text        = "FirstLine.\nSecondLine\nThirdLine";
        let initial_cursors = vec!
            [ Cursor::new(TextLocation {line:0, column:0 })
            , Cursor::new(TextLocation {line:1, column:0 })
            , Cursor::new(TextLocation {line:1, column:6 })
            , Cursor::new(TextLocation {line:1, column:10})
            , Cursor::new(TextLocation {line:2, column:9 })
            ];
        let mut expected_positions = HashMap::<Step,Vec<(usize,usize)>>::new();
        expected_positions.insert(Left,      vec![(0,0),(0,10),(1,5),(1,9),(2,8)]);
        expected_positions.insert(Right,     vec![(0,1),(1,1),(1,7),(2,0),(2,9)]);
        expected_positions.insert(Up,        vec![(0,0),(0,6),(0,10),(1,9)]);
        expected_positions.insert(Down,      vec![(1,0),(2,0),(2,6),(2,9)]);
        expected_positions.insert(LineBegin, vec![(0,0),(1,0),(2,0)]);
        expected_positions.insert(LineEnd,   vec![(0,10),(1,10),(2,9)]);
        expected_positions.insert(DocBegin,  vec![(0,0)]);
        expected_positions.insert(DocEnd,    vec![(2,9)]);

        let mut fonts      = FontRegistry::new();
        let properties     = TextFieldProperties::default(&mut fonts);
        let mut content    = TextFieldContent::new(text,&properties);
        let mut navigation = CursorNavigation {
            content: &mut content,
            selecting: false
        };

        for step in &[/*Left,Right,Up,*/Down,/*LineBegin,LineEnd,DocBegin,DocEnd*/] {
            let mut cursors = Cursors::mock(initial_cursors.clone());
            cursors.navigate_all_cursors(&mut navigation,*step);
            let expected = expected_positions.get(step).unwrap();
            let current  = cursors.cursors.iter().map(|c| (c.position.line, c.position.column));
            assert_eq!(expected,&current.collect_vec(), "Error for step {:?}", step);
        }
    }

    #[wasm_bindgen_test(async)]
    async fn moving_without_select() {
        basegl_core_msdf_sys::initialized().await;
        let text              = "FirstLine\nSecondLine";
        let initial_cursor   = Cursor {
            position    : TextLocation {line:1, column:0},
            selected_to : TextLocation {line:0, column:1}
        };
        let initial_cursors   = vec![initial_cursor];
        let new_position      = TextLocation {line:1,column:10};

        let mut fonts      = FontRegistry::new();
        let properties     = TextFieldProperties::default(&mut fonts);
        let mut content    = TextFieldContent::new(text,&properties);
        let mut navigation = CursorNavigation {
            content: &mut content,
            selecting: false
        };
        let mut cursors    = Cursors::mock(initial_cursors.clone());
        cursors.navigate_all_cursors(&mut navigation,LineEnd);
        assert_eq!(new_position, cursors.cursors.first().unwrap().position);
        assert_eq!(new_position, cursors.cursors.first().unwrap().selected_to);
    }

    #[wasm_bindgen_test(async)]
    async fn moving_with_select() {
        basegl_core_msdf_sys::initialized().await;
        let text              = "FirstLine\nSecondLine";
        let initial_loc     = TextLocation {line:0,column:1};
        let initial_cursors = vec![Cursor::new(initial_loc)];
        let new_loc         = TextLocation {line:0,column:9};

        let mut fonts      = FontRegistry::new();
        let properties     = TextFieldProperties::default(&mut fonts);
        let mut content    = TextFieldContent::new(text,&properties);
        let mut navigation = CursorNavigation {
            content: &mut content,
            selecting: true
        };
        let mut cursors = Cursors::mock(initial_cursors.clone());
        cursors.navigate_all_cursors(&mut navigation,LineEnd);
        assert_eq!(new_loc    , cursors.cursors.first().unwrap().position);
        assert_eq!(initial_loc, cursors.cursors.first().unwrap().selected_to);
    }

    #[wasm_bindgen_test(async)]
    async fn merging_selection_after_moving() {
        basegl_core_msdf_sys::initialized().await;
        let make_char_loc  = |(line,column):(usize,usize)| TextLocation {line,column};
        let cursor_on_left = |range:&Range<(usize,usize)>| Cursor {
            position    : make_char_loc(range.start),
            selected_to : make_char_loc(range.end)
        };
        let cursor_on_right = |range:&Range<(usize,usize)>| Cursor {
            position    : make_char_loc(range.end),
            selected_to : make_char_loc(range.start)
        };
        merging_selection_after_moving_case(cursor_on_left);
        merging_selection_after_moving_case(cursor_on_right);
    }

    fn merging_selection_after_moving_case<F>(convert:F)
    where F : FnMut(&Range<(usize,usize)>) -> Cursor + Clone {
        let ranges           = vec![(1,4)..(1,5), (0,0)..(0,5), (0,2)..(1,0), (1,5)..(2,0)];
        let expected_ranges  = vec![(1,4)..(1,5), (0,0)..(1,0), (1,5)..(2,0)];
        let initial_cursors  = ranges.iter().map(convert.clone()).collect_vec();
        let expected_cursors = expected_ranges.iter().map(convert).collect_vec();
        let mut cursors      = Cursors::mock(initial_cursors);

        cursors.merge_overlapping_cursors();

        assert_eq!(expected_cursors, cursors.cursors);
    }

    #[wasm_bindgen_test(async)]
    async fn recalculate_positions() {
        msdf_sys::initialized().await;
        let content     = "first sentence\r\nthis is a second sentence\r\nlast sentence\n";
        let new_content = "first sentence\r\nsecond one";
        let mut content = TextFieldContent::new(content,&mock_properties());
        let mut cursors = Cursors::default();
        cursors.cursors[0].position    = TextLocation{line:0,column:6};
        cursors.cursors[0].selected_to = TextLocation{line:0,column:14};
        cursors.add_cursor(TextLocation{line:1,column:17});
        cursors.cursors[1].selected_to = TextLocation{line:1,column:25};
        cursors.add_cursor(TextLocation{line:2,column:1});
        cursors.cursors[2].selected_to = TextLocation{line:2,column:2};
        content.set_content(new_content);
        cursors.recalculate_positions(&content);
        assert_eq!(cursors.cursors[0].position   , TextLocation{line:0,column:6});
        assert_eq!(cursors.cursors[0].selected_to, TextLocation{line:0,column:14});
        assert_eq!(cursors.cursors[1].position   , TextLocation{line:1,column:10});
        assert_eq!(cursors.cursors[1].selected_to, TextLocation{line:1,column:10});
        assert_eq!(cursors.cursors[2].position   , TextLocation{line:1,column:1});
        assert_eq!(cursors.cursors[2].selected_to, TextLocation{line:1,column:2});
    }
}
