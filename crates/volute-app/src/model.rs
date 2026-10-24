//! Frames from the audio thread, patched into the UI's models on the UI thread. Models are
//! patched (`set_row_data`, inserts, removals), never replaced (docs/ARCHITECTURE.md).

use std::time::{Duration, Instant};

use slint::{Model, SharedString, VecModel};

use crate::AppRow;
use crate::mixer::RowData;

/// How long after the user last moved a slider its row keeps the user's value, whatever an
/// older reading of the volume says.
const HOLD: Duration = Duration::from_millis(500);

/// The slider the user is moving, if any.
#[derive(Default)]
pub struct Held {
    key: Option<(SharedString, Instant)>,
}

impl Held {
    pub fn touch(&mut self, key: SharedString) {
        self.key = Some((key, Instant::now()));
    }

    fn holds(&self, key: &str) -> bool {
        self.key
            .as_ref()
            .is_some_and(|(k, at)| k == key && at.elapsed() < HOLD)
    }
}

fn to_row(data: &RowData) -> AppRow {
    AppRow {
        key: data.key.as_str().into(),
        name: data.name.as_str().into(),
        system: data.system,
        volume: data.volume,
        muted: data.muted,
        playing: data.playing,
    }
}

/// Makes `model` hold `rows`, in order, touching only what changed. A row whose slider is held
/// keeps its volume.
pub fn patch_rows(model: &VecModel<AppRow>, rows: &[RowData], held: &Held) {
    for (i, data) in rows.iter().enumerate() {
        let mut row = to_row(data);
        let current = (i < model.row_count())
            .then(|| model.row_data(i))
            .flatten();
        match current {
            Some(current) if current.key == row.key => {
                if held.holds(&row.key) || (current.volume - row.volume).abs() < 0.001 {
                    row.volume = current.volume;
                }
                if current != row {
                    model.set_row_data(i, row);
                }
            }
            _ => {
                // The row is further down (it moved up) or new.
                let later = (i + 1..model.row_count())
                    .find(|&j| model.row_data(j).is_some_and(|r| r.key == row.key));
                if let Some(j) = later {
                    let moved = model.remove(j);
                    if held.holds(&row.key) {
                        row.volume = moved.volume;
                    }
                }
                model.insert(i, row);
            }
        }
    }
    while model.row_count() > rows.len() {
        model.remove(model.row_count() - 1);
    }
}

/// Makes `model` hold `levels`, setting only the ones that moved visibly.
pub fn patch_levels(model: &VecModel<f32>, levels: &[f32]) {
    for (i, &level) in levels.iter().enumerate() {
        match model.row_data(i) {
            Some(current) if (current - level).abs() < 0.002 => {}
            Some(_) => model.set_row_data(i, level),
            None => model.push(level),
        }
    }
    while model.row_count() > levels.len() {
        model.remove(model.row_count() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(key: &str, volume: f32) -> RowData {
        RowData {
            key: key.into(),
            name: key.to_uppercase(),
            system: false,
            volume,
            muted: false,
            playing: true,
        }
    }

    fn keys(model: &VecModel<AppRow>) -> Vec<String> {
        model.iter().map(|r| r.key.to_string()).collect()
    }

    #[test]
    fn patching_produces_the_rows_in_order() {
        let model = VecModel::<AppRow>::default();
        let held = Held::default();
        patch_rows(&model, &[data("a", 0.1), data("b", 0.2), data("c", 0.3)], &held);
        assert_eq!(keys(&model), ["a", "b", "c"]);

        // c moves up, b goes, d arrives, a changes.
        let next = [data("c", 0.3), data("a", 0.9), data("d", 0.4)];
        patch_rows(&model, &next, &held);
        assert_eq!(keys(&model), ["c", "a", "d"]);
        let expected: Vec<AppRow> = next.iter().map(to_row).collect();
        assert_eq!(model.iter().collect::<Vec<_>>(), expected);

        patch_rows(&model, &[], &held);
        assert_eq!(model.row_count(), 0);
    }

    #[test]
    fn a_held_slider_keeps_the_users_value() {
        let model = VecModel::<AppRow>::default();
        let mut held = Held::default();
        patch_rows(&model, &[data("a", 0.5), data("b", 0.5)], &held);
        held.touch("a".into());
        patch_rows(&model, &[data("a", 0.2), data("b", 0.2)], &held);
        assert_eq!(model.row_data(0).unwrap().volume, 0.5, "held: the user's value");
        assert_eq!(model.row_data(1).unwrap().volume, 0.2, "not held: the new reading");
    }

    #[test]
    fn levels_follow_the_rows() {
        let model = VecModel::<f32>::default();
        patch_levels(&model, &[0.1, 0.2, 0.3]);
        patch_levels(&model, &[0.5, 0.2]);
        assert_eq!(model.iter().collect::<Vec<_>>(), [0.5, 0.2]);
    }
}
