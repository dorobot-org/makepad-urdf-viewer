//! Joint control panel — the sidebar every usable URDF viewer has.
//!
//! One row per movable joint: name, live value, a slider bounded by the URDF
//! limits, and the limits themselves. Rows are recycled through a `PortalList`,
//! so a 32-joint model costs the same as a 6-joint one.
//!
//! It owns no robot state. The host feeds it rows and receives
//! [`JointPanelAction::Changed`] when the user drags something.

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.draw

    mod.widgets.JointPanelBase = #(JointPanel::register_widget(vm))
    mod.widgets.JointPanel = set_type_default() do mod.widgets.JointPanelBase{
        width: Fill
        height: Fill

        flow: Down

        list := PortalList{
            width: Fill
            height: Fill
            flow: Down
            drag_scrolling: false
            scroll_bar: ScrollBar{}

            JointRow := View{
                width: Fill
                height: Fit
                flow: Down
                padding: Inset{left: 12.0 right: 12.0 top: 7.0 bottom: 8.0}
                spacing: 2.0

                head := View{
                    width: Fill
                    height: Fit
                    flow: Right
                    align: Align{y: 0.5}
                    name := Label{
                        text: "joint"
                        draw_text +: {color: #xdfe6f2}
                    }
                    filler := View{width: Fill height: 1.0}
                    value := Label{
                        text: "+0.000"
                        draw_text +: {color: #x6ba4f8}
                    }
                }

                // Normalised 0..1: PortalList items come from one template, so
                // a per-joint min/max is not settable. The row maps to and from
                // the joint's real range, and shows radians in `value`.
                slider := Slider{
                    width: Fill
                    text: ""
                    min: 0.0
                    max: 1.0
                    default: 0.5
                    text_input: TextInput{ width: 0.0 visible: false }
                }

                limits := Label{
                    text: ""
                    draw_text +: {color: #x606b7d}
                }
            }
        }
    }
}

/// One movable joint, as the panel needs to show it.
#[derive(Clone, Debug, Default)]
pub struct JointRow {
    pub name: String,
    pub value: f32,
    pub lower: f32,
    pub upper: f32,
    /// continuous joints have no meaningful limits; the slider spans ±π
    pub continuous: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum JointPanelAction {
    #[default]
    None,
    /// The user moved a joint. `index` is into the rows last supplied.
    Changed { index: usize, value: f32 },
}

#[derive(Script, ScriptHook, Widget)]
pub struct JointPanel {
    /// the component IS a view; PortalList hangs off it and is yielded as a
    /// draw step (a plain WidgetRef does not defer it — verified)
    #[deref]
    view: View,
    #[rust]
    rows: Vec<JointRow>,
    /// row currently highlighted (kept in step with the 3D view's selection)
    #[rust(0usize)]
    selected: usize,
    /// true while a slider is being dragged, so incoming set_values does not
    /// fight the user's hand
    #[rust(false)]
    dragging: bool,
}

impl JointRow {
    /// position on a 0..1 slider
    fn norm(&self) -> f32 {
        let (lo, hi) = (self.lower, self.upper);
        if hi <= lo {
            return 0.5;
        }
        ((self.value - lo) / (hi - lo)).clamp(0.0, 1.0)
    }

    /// slider position back to radians
    fn denorm(&self, t: f32) -> f32 {
        let (lo, hi) = (self.lower, self.upper);
        lo + t.clamp(0.0, 1.0) * (hi - lo)
    }
}

impl JointPanel {
    fn label_for(row: &JointRow) -> String {
        if row.continuous {
            "continuous".to_string()
        } else {
            format!("{:+.2}   {:+.2}", row.lower, row.upper)
        }
    }
}

impl Widget for JointPanel {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.as_portal_list().borrow_mut() {
                list.set_item_range(cx, 0, self.rows.len());
                while let Some(i) = list.next_visible_item(cx) {
                    let Some(row) = self.rows.get(i) else { continue };
                    let item = list.item(cx, i, id!(JointRow));
                    item.label(cx, ids!(name)).set_text(cx, &row.name);
                    item.label(cx, ids!(value))
                        .set_text(cx, &format!("{:+.3}", row.value));
                    item.label(cx, ids!(limits))
                        .set_text(cx, &Self::label_for(row));
                    let slider = item.slider(cx, ids!(slider));
                    if !self.dragging {
                        slider.set_value(cx, row.norm() as f64);
                    }
                    item.draw_all_unscoped(cx);
                }
            }
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Sliders live inside recycled PortalList items, so their actions are
        // collected here and mapped back to a row index.
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });
        if actions.is_empty() {
            return;
        }
        let list_ref = self.view.portal_list(cx, ids!(list));
        let mut changed: Option<(usize, f32)> = None;
        {
            let Some(list) = list_ref.borrow() else { return };
            for i in 0..self.rows.len() {
                let Some(item) = list.get_item(i) else { continue };
                let slider = item.1.slider(cx, ids!(slider));
                if let Some(v) = slider.slided(&actions) {
                    changed = Some((i, self.rows[i].denorm(v as f32)));
                }
                if slider.end_slide(&actions).is_some() {
                    self.dragging = false;
                }
            }
        }
        if let Some((index, value)) = changed {
            self.dragging = true;
            self.selected = index;
            if let Some(row) = self.rows.get_mut(index) {
                row.value = value;
            }
            cx.widget_action(self.widget_uid(), JointPanelAction::Changed { index, value });
            self.view.redraw(cx);
        }
    }
}

impl JointPanelRef {
    /// Replace the joint list. Call on load, or when the model changes.
    pub fn set_joints(&self, cx: &mut Cx, rows: Vec<JointRow>) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.rows = rows;
            inner.selected = 0;
            inner.view.redraw(cx);
        }
    }

    /// Update the displayed values without rebuilding the list — for playback
    /// or the built-in animation.
    pub fn set_values(&self, cx: &mut Cx, values: &[f32]) {
        if let Some(mut inner) = self.borrow_mut() {
            for (row, v) in inner.rows.iter_mut().zip(values) {
                row.value = *v;
            }
            inner.view.redraw(cx);
        }
    }

    /// Highlight the row the 3D view considers selected.
    pub fn set_selected(&self, cx: &mut Cx, index: usize) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.selected = index;
            inner.view.redraw(cx);
        }
    }

    /// `Some((index, value))` on the frame a slider moved.
    pub fn changed(&self, actions: &Actions) -> Option<(usize, f32)> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let JointPanelAction::Changed { index, value } = item.cast() {
                return Some((index, value));
            }
        }
        None
    }

    pub fn joint_count(&self) -> usize {
        self.borrow().map(|i| i.rows.len()).unwrap_or(0)
    }
}
