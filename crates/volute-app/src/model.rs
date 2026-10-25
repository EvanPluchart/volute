//! Frames from the audio thread, patched into the UI's models on the UI thread. Models are
//! patched (`set_row_data`, inserts, removals), never replaced (docs/ARCHITECTURE.md).

use std::time::{Duration, Instant};

use slint::{Model, SharedString, VecModel};

use crate::mixer::{DeviceData, RowData};
use crate::{AppRow, DeviceRow};

/// How long after the user last moved a slider its row keeps the user's value, whatever an
/// older reading of the volume says.
const HOLD: Duration = Duration::from_millis(500);

/// The slider the user is moving, if any (an app's key or a device's id).
#[derive(Default)]
pub struct Held {
    key: Option<(SharedString, Instant)>,
}

impl Held {
    pub fn touch(&mut self, key: SharedString) {
        self.key = Some((key, Instant::now()));
    }

    pub fn holds(&self, key: &str) -> bool {
        self.key
            .as_ref()
            .is_some_and(|(k, at)| k == key && at.elapsed() < HOLD)
    }
}

pub fn to_row(data: &RowData) -> AppRow {
    AppRow {
        key: data.key.as_str().into(),
        name: data.name.as_str().into(),
        initial: data.name.chars().find(|c| c.is_alphanumeric()).map_or("?".into(), |c| c.to_uppercase().collect::<String>()).as_str().into(),
        system: data.system,
        volume: data.volume,
        muted: data.muted,
        playing: data.playing,
        icon: data.icon,
    }
}

pub fn to_device(data: &DeviceData) -> DeviceRow {
    DeviceRow {
        id: data.id.as_str().into(),
        name: data.name.as_str().into(),
        output: data.output,
        volume: data.volume,
        muted: data.muted,
        default: data.default,
    }
}

/// Makes `model` hold `rows`, in order, touching only what changed. `keep` may carry values over
/// from the current row (a slider the user holds keeps its volume).
fn patch_keyed<T: Clone + PartialEq + 'static>(
    model: &VecModel<T>,
    rows: Vec<T>,
    key: impl Fn(&T) -> SharedString,
    keep: impl Fn(&T, &mut T),
) {
    for (i, mut row) in rows.iter().cloned().enumerate() {
        let k = key(&row);
        let current = (i < model.row_count()).then(|| model.row_data(i)).flatten();
        match current {
            Some(current) if key(&current) == k => {
                keep(&current, &mut row);
                if current != row {
                    model.set_row_data(i, row);
                }
            }
            _ => {
                // The row is further down (it moved up) or new.
                let later = (i + 1..model.row_count())
                    .find(|&j| model.row_data(j).is_some_and(|r| key(&r) == k));
                if let Some(j) = later {
                    let moved = model.remove(j);
                    keep(&moved, &mut row);
                }
                model.insert(i, row);
            }
        }
    }
    while model.row_count() > rows.len() {
        model.remove(model.row_count() - 1);
    }
}

/// The app rows. A row whose slider is held keeps its volume, and so does one whose new
/// reading is the same value seen through the curve's rounding.
pub fn patch_rows(model: &VecModel<AppRow>, rows: &[RowData], held: &Held) {
    patch_keyed(
        model,
        rows.iter().map(to_row).collect(),
        |r| r.key.clone(),
        |current, new| {
            if held.holds(&new.key) || (current.volume - new.volume).abs() < 0.001 {
                new.volume = current.volume;
            }
        },
    );
}

/// The devices, outputs and inputs, same rules.
pub fn patch_devices(model: &VecModel<DeviceRow>, devices: &[DeviceData], held: &Held) {
    patch_keyed(
        model,
        devices.iter().map(to_device).collect(),
        |d| d.id.clone(),
        |current, new| {
            if held.holds(&new.id) || (current.volume - new.volume).abs() < 0.001 {
                new.volume = current.volume;
            }
        },
    );
}

/// Makes `model` hold `levels`, setting only the ones that moved visibly.
pub fn patch_levels(model: &VecModel<f32>, levels: &[f32]) {
    for (i, &level) in levels.iter().enumerate() {
        match model.row_data(i) {
            Some(current) if (current - level).abs() < 0.004 => {}
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
            icon: -1,
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
    fn devices_patch_by_id() {
        let model = VecModel::<DeviceRow>::default();
        let held = Held::default();
        let device = |id: &str, default: bool| DeviceData {
            id: id.into(),
            name: id.into(),
            output: true,
            volume: 0.5,
            muted: false,
            default,
        };
        patch_devices(&model, &[device("a", true), device("b", false)], &held);
        patch_devices(&model, &[device("b", true), device("a", false)], &held);
        let ids: Vec<(String, bool)> = model.iter().map(|d| (d.id.to_string(), d.default)).collect();
        assert_eq!(ids, [("b".to_owned(), true), ("a".to_owned(), false)]);
    }

    #[test]
    fn levels_follow_the_rows() {
        let model = VecModel::<f32>::default();
        patch_levels(&model, &[0.1, 0.2, 0.3]);
        patch_levels(&model, &[0.5, 0.2]);
        assert_eq!(model.iter().collect::<Vec<_>>(), [0.5, 0.2]);
    }
}
