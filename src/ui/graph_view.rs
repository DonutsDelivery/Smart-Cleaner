use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::gio;

use crate::model::dep_tree::DependencyTree;

use super::graph_layout::{GraphLayout, NODE_HEIGHT, NODE_WIDTH};
use super::graph_node;

/// Holds the graph canvas and its state.
pub struct GraphView {
    pub widget: gtk::ScrolledWindow,
    pub drawing_area: gtk::DrawingArea,
    pub layout: Rc<RefCell<Option<GraphLayout>>>,
    pub hovered_node: Rc<RefCell<Option<String>>>,
    pub search_matches: Rc<RefCell<HashSet<String>>>,
    pub search_active: Rc<RefCell<bool>>,
}

impl GraphView {
    pub fn new(
        dep_tree: Rc<RefCell<Option<DependencyTree>>>,
        selected_ids: Rc<RefCell<HashSet<String>>>,
        on_selection_changed: impl Fn() + Clone + 'static,
    ) -> Self {
        let drawing_area = gtk::DrawingArea::new();
        let layout: Rc<RefCell<Option<GraphLayout>>> = Rc::new(RefCell::new(None));
        let hovered_node: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let search_matches: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));
        let search_active: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

        // Icon cache: maps icon_name → pre-loaded texture
        let icon_cache: Rc<RefCell<HashMap<String, Option<gtk::gdk::Texture>>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // Set up draw function
        {
            let layout = layout.clone();
            let dep_tree = dep_tree.clone();
            let selected_ids = selected_ids.clone();
            let hovered = hovered_node.clone();
            let matches = search_matches.clone();
            let searching = search_active.clone();
            let icons = icon_cache.clone();

            drawing_area.set_draw_func(move |da, cr, _w, _h| {
                let layout_ref = layout.borrow();
                let Some(ref layout) = *layout_ref else {
                    return;
                };

                let tree_ref = dep_tree.borrow();
                let selected = selected_ids.borrow();
                let hover = hovered.borrow();
                let search = matches.borrow();
                let is_searching = *searching.borrow();

                // Visible region for culling
                let (vis_x, vis_y, vis_w, vis_h) = visible_region(da);

                // Background
                cr.set_source_rgb(0.12, 0.12, 0.14);
                cr.paint().ok();

                // Draw edges as smooth curves
                for (from_id, to_id) in &layout.edges {
                    if let (Some(&(fx, fy)), Some(&(tx, ty))) = (
                        layout.node_positions.get(from_id),
                        layout.node_positions.get(to_id),
                    ) {
                        // Cull edges outside visible area
                        let edge_min_x = fx.min(tx) - NODE_WIDTH;
                        let edge_max_x = fx.max(tx) + NODE_WIDTH;
                        let edge_min_y = fy.min(ty) - NODE_HEIGHT;
                        let edge_max_y = fy.max(ty) + NODE_HEIGHT;
                        if edge_max_x < vis_x
                            || edge_min_x > vis_x + vis_w
                            || edge_max_y < vis_y
                            || edge_min_y > vis_y + vis_h
                        {
                            continue;
                        }

                        let from_selected = selected.contains(from_id);
                        let to_selected = selected.contains(to_id);
                        if from_selected && to_selected {
                            cr.set_source_rgba(0.35, 0.55, 0.95, 0.8);
                            cr.set_line_width(2.0);
                        } else if is_searching
                            && (search.contains(from_id) || search.contains(to_id))
                        {
                            cr.set_source_rgba(0.9, 0.7, 0.2, 0.5);
                            cr.set_line_width(1.0);
                        } else {
                            cr.set_source_rgba(0.4, 0.4, 0.45, 0.25);
                            cr.set_line_width(0.8);
                        }

                        // Bezier curve between nodes (auto-detect direction)
                        let (lx, ly, rx, ry) = if fx < tx {
                            (fx + NODE_WIDTH / 2.0, fy, tx - NODE_WIDTH / 2.0, ty)
                        } else {
                            (tx + NODE_WIDTH / 2.0, ty, fx - NODE_WIDTH / 2.0, fy)
                        };
                        let ctrl_dx = (rx - lx).abs() * 0.5;

                        cr.move_to(lx, ly);
                        cr.curve_to(lx + ctrl_dx, ly, rx - ctrl_dx, ry, rx, ry);
                        cr.stroke().ok();
                    }
                }

                // Load icon theme once
                let icon_theme = gtk::IconTheme::for_display(&da.display());
                let mut icon_cache = icons.borrow_mut();

                // Draw nodes
                for (id, &(cx, cy)) in &layout.node_positions {
                    let left = cx - NODE_WIDTH / 2.0;
                    let top = cy - NODE_HEIGHT / 2.0;

                    // Cull nodes outside visible area
                    if left > vis_x + vis_w + 10.0
                        || left + NODE_WIDTH < vis_x - 10.0
                        || top > vis_y + vis_h + 10.0
                        || top + NODE_HEIGHT < vis_y - 10.0
                    {
                        continue;
                    }

                    let is_selected = selected.contains(id);
                    let is_hovered = hover.as_deref().is_some_and(|h| h == id);
                    let is_match = is_searching && search.contains(id);
                    let is_dimmed = is_searching && !is_match;

                    // Get display info + icon
                    let (display_name, is_explicit, icon_name) =
                        if let Some(ref tree) = *tree_ref {
                            if let Some(pkg) = tree.get(id) {
                                let dn = if pkg.display_name.is_empty() {
                                    &pkg.name
                                } else {
                                    &pkg.display_name
                                };
                                (dn.to_string(), pkg.is_explicit, pkg.icon_name.clone())
                            } else {
                                (id.clone(), false, None)
                            }
                        } else {
                            (id.clone(), false, None)
                        };

                    // Node background
                    let radius = 6.0;
                    rounded_rect(cr, left, top, NODE_WIDTH, NODE_HEIGHT, radius);

                    if is_selected {
                        cr.set_source_rgba(0.25, 0.45, 0.85, 0.9);
                    } else if is_hovered {
                        cr.set_source_rgba(0.3, 0.3, 0.35, 0.95);
                    } else if is_match {
                        cr.set_source_rgba(0.35, 0.3, 0.15, 0.9);
                    } else if is_dimmed {
                        cr.set_source_rgba(0.18, 0.18, 0.2, 0.5);
                    } else if is_explicit {
                        cr.set_source_rgba(0.22, 0.22, 0.26, 0.95);
                    } else {
                        cr.set_source_rgba(0.17, 0.17, 0.2, 0.85);
                    }
                    cr.fill().ok();

                    // Node border
                    rounded_rect(cr, left, top, NODE_WIDTH, NODE_HEIGHT, radius);
                    if is_selected {
                        cr.set_source_rgba(0.4, 0.6, 1.0, 1.0);
                        cr.set_line_width(2.0);
                    } else if is_hovered {
                        cr.set_source_rgba(0.6, 0.6, 0.65, 0.8);
                        cr.set_line_width(1.5);
                    } else if is_match {
                        cr.set_source_rgba(0.9, 0.7, 0.2, 0.8);
                        cr.set_line_width(1.5);
                    } else if is_explicit {
                        cr.set_source_rgba(0.4, 0.4, 0.45, 0.6);
                        cr.set_line_width(1.0);
                    } else {
                        cr.set_source_rgba(0.3, 0.3, 0.33, 0.4);
                        cr.set_line_width(0.6);
                    }
                    cr.stroke().ok();

                    // Icon (left side, 14x14)
                    let icon_size = 14.0;
                    let icon_x = left + 4.0;
                    let icon_y = cy - icon_size / 2.0;
                    let mut has_icon = false;

                    if let Some(ref iname) = icon_name {
                        if !iname.is_empty() {
                            let texture = icon_cache
                                .entry(iname.clone())
                                .or_insert_with(|| {
                                    load_icon_texture(&icon_theme, iname, icon_size as i32)
                                })
                                .clone();

                            if let Some(ref tex) = texture {
                                cr.save().ok();
                                let tw = tex.width() as f64;
                                let th = tex.height() as f64;
                                if tw > 0.0 && th > 0.0 {
                                    let scale = (icon_size / tw).min(icon_size / th);
                                    let ox = icon_x + (icon_size - tw * scale) / 2.0;
                                    let oy = icon_y + (icon_size - th * scale) / 2.0;
                                    cr.translate(ox, oy);
                                    cr.scale(scale, scale);
                                    // Render texture to cairo via snapshot
                                    // Fallback: draw a colored square for now
                                    has_icon = false; // texture painting requires gdk_cairo which we don't have
                                }
                                cr.restore().ok();
                            }
                        }
                    }

                    // Fallback icon indicator: small colored dot
                    if !has_icon {
                        let dot_r = 3.0;
                        let dot_cx = icon_x + icon_size / 2.0;
                        let dot_cy = cy;
                        cr.arc(dot_cx, dot_cy, dot_r, 0.0, 2.0 * std::f64::consts::PI);
                        if is_explicit {
                            cr.set_source_rgba(0.3, 0.65, 0.4, 0.8);
                        } else {
                            cr.set_source_rgba(0.45, 0.45, 0.5, 0.6);
                        }
                        cr.fill().ok();
                    }

                    // Package name text
                    let text_x = icon_x + icon_size + 4.0;
                    let max_text_width = NODE_WIDTH - (text_x - left) - 4.0;

                    if is_dimmed {
                        cr.set_source_rgba(0.6, 0.6, 0.63, 0.4);
                    } else if is_selected {
                        cr.set_source_rgb(1.0, 1.0, 1.0);
                    } else if is_explicit {
                        cr.set_source_rgba(0.9, 0.9, 0.92, 0.95);
                    } else {
                        cr.set_source_rgba(0.75, 0.75, 0.78, 0.9);
                    }

                    cr.set_font_size(10.0);
                    let mut label = display_name;
                    loop {
                        let Ok(extents) = cr.text_extents(&label) else {
                            break;
                        };
                        if extents.width() <= max_text_width || label.len() <= 3 {
                            break;
                        }
                        label.pop();
                        label.pop();
                        label.push('…');
                    }
                    cr.move_to(text_x, cy + 4.0);
                    cr.show_text(&label).ok();
                }
            });
        }

        // Mouse motion controller for hover (NO popover — just highlight)
        let motion_controller = gtk::EventControllerMotion::new();
        {
            let hovered = hovered_node.clone();
            let layout = layout.clone();
            let da = drawing_area.clone();

            motion_controller.connect_motion(move |_ctrl, x, y| {
                let layout_ref = layout.borrow();
                let new_hover = if let Some(ref l) = *layout_ref {
                    l.hit_test(x, y).map(String::from)
                } else {
                    None
                };

                let old_hover = hovered.borrow().clone();
                if new_hover != old_hover {
                    *hovered.borrow_mut() = new_hover;
                    da.queue_draw();
                }
            });

            let hovered_leave = hovered_node.clone();
            let da_leave = drawing_area.clone();
            motion_controller.connect_leave(move |_ctrl| {
                *hovered_leave.borrow_mut() = None;
                da_leave.queue_draw();
            });
        }
        drawing_area.add_controller(motion_controller);

        // Right-click to show popover with package details (stable, no flicker)
        let right_click = gtk::GestureClick::new();
        right_click.set_button(3); // right button
        {
            let layout = layout.clone();
            let dep_tree_rc = dep_tree.clone();
            let da = drawing_area.clone();
            let active_popover: Rc<RefCell<Option<gtk::Popover>>> = Rc::new(RefCell::new(None));

            right_click.connect_released(move |_gesture, _n, x, y| {
                // Dismiss any existing popover
                if let Some(ref p) = *active_popover.borrow() {
                    p.popdown();
                    p.unparent();
                }
                *active_popover.borrow_mut() = None;

                let layout_ref = layout.borrow();
                let Some(ref l) = *layout_ref else { return };
                let Some(node_id) = l.hit_test(x, y) else {
                    return;
                };

                if let Some((left, top, w, _h)) = l.node_rect(node_id) {
                    let tree_ref = dep_tree_rc.borrow();
                    if let Some(ref tree) = *tree_ref {
                        if let Some(pkg) = tree.get(node_id) {
                            let p = graph_node::create_popover(&da, pkg, tree, left + w / 2.0, top);
                            p.popup();
                            *active_popover.borrow_mut() = Some(p);
                        }
                    }
                }
            });
        }
        drawing_area.add_controller(right_click);

        // Left-click controller for selection toggle
        let click_controller = gtk::GestureClick::new();
        {
            let layout = layout.clone();
            let dep_tree = dep_tree.clone();
            let selected_ids = selected_ids.clone();
            let da = drawing_area.clone();
            let on_changed = on_selection_changed.clone();

            click_controller.connect_released(move |_gesture, _n_press, x, y| {
                let layout_ref = layout.borrow();
                let Some(ref l) = *layout_ref else { return };

                if let Some(node_id) = l.hit_test(x, y) {
                    let node_id = node_id.to_string();
                    let mut sel = selected_ids.borrow_mut();

                    if sel.contains(&node_id) {
                        sel.remove(&node_id);
                    } else {
                        sel.insert(node_id.clone());

                        // Auto-select orphaned deps
                        if let Some(ref tree) = *dep_tree.borrow() {
                            let branch = tree.compute_removal_branch(&sel);
                            *sel = branch;
                        }
                    }
                    drop(sel);
                    on_changed();
                    da.queue_draw();
                }
            });
        }
        drawing_area.add_controller(click_controller);

        // Scroll window
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&drawing_area));
        scroll.set_vexpand(true);
        scroll.set_hexpand(true);

        Self {
            widget: scroll,
            drawing_area,
            layout,
            hovered_node,
            search_matches,
            search_active,
        }
    }

    /// Update the layout from a freshly scanned dependency tree.
    pub fn set_layout(&self, new_layout: GraphLayout) {
        self.drawing_area
            .set_content_width(new_layout.canvas_width as i32);
        self.drawing_area
            .set_content_height(new_layout.canvas_height as i32);
        *self.layout.borrow_mut() = Some(new_layout);
        self.drawing_area.queue_draw();
    }

    /// Update search highlighting.
    pub fn set_search(&self, query: &str, tree: &DependencyTree) {
        let mut matches = self.search_matches.borrow_mut();
        matches.clear();

        if query.is_empty() {
            *self.search_active.borrow_mut() = false;
        } else {
            *self.search_active.borrow_mut() = true;
            let query_lower = query.to_lowercase();

            for pkg in tree.all_packages() {
                let name_lower = pkg.name.to_lowercase();
                let display_lower = pkg.display_name.to_lowercase();
                if name_lower.contains(&query_lower) || display_lower.contains(&query_lower) {
                    matches.insert(pkg.qualified_id());
                }
            }
        }

        self.drawing_area.queue_draw();
    }

    /// Force redraw.
    pub fn queue_draw(&self) {
        self.drawing_area.queue_draw();
    }
}

use std::collections::HashMap;

/// Get the visible scroll region of a DrawingArea inside a ScrolledWindow.
fn visible_region(da: &gtk::DrawingArea) -> (f64, f64, f64, f64) {
    if let Some(parent) = da.parent() {
        if let Some(sw) = parent.downcast_ref::<gtk::ScrolledWindow>() {
            let h = sw.hadjustment();
            let v = sw.vadjustment();
            return (h.value(), v.value(), h.page_size(), v.page_size());
        }
    }
    (0.0, 0.0, da.width() as f64, da.height() as f64)
}

/// Try to load an icon as a GDK texture from the icon theme.
fn load_icon_texture(
    theme: &gtk::IconTheme,
    icon_name: &str,
    size: i32,
) -> Option<gtk::gdk::Texture> {
    if !theme.has_icon(icon_name) {
        return None;
    }
    let icon_paintable = theme.lookup_icon(
        icon_name,
        &[],
        size,
        1,
        gtk::TextDirection::None,
        gtk::IconLookupFlags::empty(),
    );
    // Try to get file path and load as texture
    if let Some(file) = icon_paintable.file() {
        if let Some(path) = file.path() {
            return gtk::gdk::Texture::from_file(&gio::File::for_path(&path)).ok();
        }
    }
    None
}

/// Draw a rounded rectangle path.
fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}
