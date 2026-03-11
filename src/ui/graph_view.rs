use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::gio;

use crate::model::dep_tree::DependencyTree;

use super::graph_layout::{
    category_color, primary_category, GraphFilter, GraphLayout, GraphSort, CATEGORY_LABELS,
    NODE_HEIGHT, NODE_WIDTH,
};
use super::graph_node;

/// Holds the graph canvas and its state.
pub struct GraphView {
    pub widget: gtk::Box,
    pub drawing_area: gtk::DrawingArea,
    pub layout: Rc<RefCell<Option<GraphLayout>>>,
    pub hovered_node: Rc<RefCell<Option<String>>>,
    pub search_matches: Rc<RefCell<HashSet<String>>>,
    pub search_active: Rc<RefCell<bool>>,
    /// Visual-only: deps that pacman -Rns would auto-remove (not in selected_ids)
    pub orphan_preview: Rc<RefCell<HashSet<String>>>,
    dep_tree: Rc<RefCell<Option<DependencyTree>>>,
    filter: Rc<Cell<GraphFilter>>,
    sort: Rc<Cell<GraphSort>>,
    depth: Rc<Cell<usize>>,
    max_dep_fans: Rc<Cell<usize>>,
    category_idx: Rc<Cell<usize>>, // 0 = All, 1..=N maps to CATEGORY_LABELS
    zoom: Rc<Cell<f64>>,
    scroll_window: gtk::ScrolledWindow,
    count_label: gtk::Label,
}

impl GraphView {
    pub fn new(
        dep_tree: Rc<RefCell<Option<DependencyTree>>>,
        selected_ids: Rc<RefCell<HashSet<String>>>,
        explicit_ids: Rc<RefCell<HashSet<String>>>,
        on_selection_changed: impl Fn() + Clone + 'static,
    ) -> Self {
        let drawing_area = gtk::DrawingArea::new();
        let layout: Rc<RefCell<Option<GraphLayout>>> = Rc::new(RefCell::new(None));
        let hovered_node: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let search_matches: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));
        let search_active: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let orphan_preview: Rc<RefCell<HashSet<String>>> =
            Rc::new(RefCell::new(HashSet::new()));

        let filter: Rc<Cell<GraphFilter>> = Rc::new(Cell::new(GraphFilter::default()));
        let sort: Rc<Cell<GraphSort>> = Rc::new(Cell::new(GraphSort::default()));
        let depth: Rc<Cell<usize>> = Rc::new(Cell::new(1));
        let max_dep_fans: Rc<Cell<usize>> = Rc::new(Cell::new(usize::MAX));
        let category_idx: Rc<Cell<usize>> = Rc::new(Cell::new(0));
        let zoom: Rc<Cell<f64>> = Rc::new(Cell::new(1.0));

        // Icon cache: maps icon_name -> pre-loaded cairo surface
        let icon_cache: Rc<RefCell<HashMap<String, Option<gtk::cairo::ImageSurface>>>> =
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
            let zoom_draw = zoom.clone();
            let orphans_draw = orphan_preview.clone();

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
                let orphans = orphans_draw.borrow();
                let z = zoom_draw.get();

                // Visible region for culling (in canvas coordinates)
                let (sv_x, sv_y, sv_w, sv_h) = visible_region(da);
                let vis_x = sv_x / z;
                let vis_y = sv_y / z;
                let vis_w = sv_w / z;
                let vis_h = sv_h / z;

                // Background (full widget area, before zoom transform)
                cr.set_source_rgb(0.12, 0.12, 0.14);
                cr.paint().ok();

                // Apply zoom transform
                cr.scale(z, z);

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
                            cr.set_line_width(3.0);
                        } else if is_searching
                            && (search.contains(from_id) || search.contains(to_id))
                        {
                            cr.set_source_rgba(0.9, 0.7, 0.2, 0.5);
                            cr.set_line_width(1.5);
                        } else {
                            cr.set_source_rgba(0.5, 0.5, 0.55, 0.6);
                            cr.set_line_width(1.5);
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
                    let is_orphan = orphans.contains(id);
                    let is_hovered = hover.as_deref().is_some_and(|h| h == id);
                    let is_match = is_searching && search.contains(id);
                    let is_dimmed = is_searching && !is_match;

                    // Get display info + icon + categories + protection
                    let (display_name, is_explicit, is_protected, icon_name, cat_label) =
                        if let Some(ref tree) = *tree_ref {
                            if let Some(pkg) = tree.get(id) {
                                let dn = if pkg.display_name.is_empty() {
                                    &pkg.name
                                } else {
                                    &pkg.display_name
                                };
                                let cl = primary_category(&pkg.categories);
                                (dn.to_string(), pkg.is_explicit, pkg.is_protected, pkg.icon_name.clone(), cl)
                            } else {
                                (id.clone(), false, false, None, "")
                            }
                        } else {
                            (id.clone(), false, false, None, "")
                        };

                    // Node background — use category color for explicit packages
                    let radius = 9.0;
                    rounded_rect(cr, left, top, NODE_WIDTH, NODE_HEIGHT, radius);

                    if is_selected {
                        cr.set_source_rgba(0.25, 0.45, 0.85, 0.9);
                    } else if is_orphan {
                        // Orphan preview: dimmer red/orange tint
                        cr.set_source_rgba(0.35, 0.18, 0.15, 0.85);
                    } else if is_hovered {
                        cr.set_source_rgba(0.3, 0.3, 0.35, 0.95);
                    } else if is_match {
                        cr.set_source_rgba(0.35, 0.3, 0.15, 0.9);
                    } else if is_dimmed {
                        cr.set_source_rgba(0.18, 0.18, 0.2, 0.5);
                    } else if is_explicit && !cat_label.is_empty() {
                        let (r, g, b, a) = category_color(cat_label);
                        cr.set_source_rgba(r, g, b, a);
                    } else if is_explicit {
                        cr.set_source_rgba(0.22, 0.22, 0.26, 0.95);
                    } else {
                        cr.set_source_rgba(0.17, 0.17, 0.2, 0.85);
                    }
                    cr.fill().ok();

                    // Node border
                    rounded_rect(cr, left, top, NODE_WIDTH, NODE_HEIGHT, radius);
                    if is_protected {
                        cr.set_source_rgba(0.6, 0.5, 0.2, 0.8);
                        cr.set_line_width(2.0);
                    } else if is_selected {
                        cr.set_source_rgba(0.4, 0.6, 1.0, 1.0);
                        cr.set_line_width(3.0);
                    } else if is_orphan {
                        cr.set_source_rgba(0.7, 0.35, 0.25, 0.7);
                        cr.set_line_width(2.0);
                    } else if is_hovered {
                        cr.set_source_rgba(0.6, 0.6, 0.65, 0.8);
                        cr.set_line_width(2.25);
                    } else if is_match {
                        cr.set_source_rgba(0.9, 0.7, 0.2, 0.8);
                        cr.set_line_width(2.25);
                    } else if is_explicit {
                        cr.set_source_rgba(0.4, 0.4, 0.45, 0.6);
                        cr.set_line_width(1.5);
                    } else {
                        cr.set_source_rgba(0.3, 0.3, 0.33, 0.4);
                        cr.set_line_width(0.9);
                    }
                    cr.stroke().ok();

                    // Icon (left side, 21x21)
                    let icon_size = 21.0;
                    let icon_x = left + 6.0;
                    let icon_y = cy - icon_size / 2.0;
                    let mut has_icon = false;

                    if let Some(ref iname) = icon_name {
                        if !iname.is_empty() {
                            let surface = icon_cache
                                .entry(iname.clone())
                                .or_insert_with(|| {
                                    load_icon_surface(&icon_theme, iname, icon_size as i32)
                                })
                                .clone();

                            if let Some(ref surf) = surface {
                                let sw = surf.width() as f64;
                                let sh = surf.height() as f64;
                                if sw > 0.0 && sh > 0.0 {
                                    cr.save().ok();
                                    let scale = (icon_size / sw).min(icon_size / sh);
                                    let ox = icon_x + (icon_size - sw * scale) / 2.0;
                                    let oy = icon_y + (icon_size - sh * scale) / 2.0;
                                    cr.translate(ox, oy);
                                    cr.scale(scale, scale);
                                    cr.set_source_surface(surf, 0.0, 0.0).ok();
                                    cr.paint().ok();
                                    cr.restore().ok();
                                    has_icon = true;
                                }
                            }
                        }
                    }

                    // Fallback icon indicator: small colored dot
                    if !has_icon {
                        let dot_r = 4.5;
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
                    let text_x = icon_x + icon_size + 6.0;
                    let max_text_width = NODE_WIDTH - (text_x - left) - 6.0;

                    if is_dimmed {
                        cr.set_source_rgba(0.6, 0.6, 0.63, 0.4);
                    } else if is_selected {
                        cr.set_source_rgb(1.0, 1.0, 1.0);
                    } else if is_explicit {
                        cr.set_source_rgba(0.9, 0.9, 0.92, 0.95);
                    } else {
                        cr.set_source_rgba(0.75, 0.75, 0.78, 0.9);
                    }

                    cr.set_font_size(15.0);
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
                        label.push('\u{2026}');
                    }
                    cr.move_to(text_x, cy + 6.0);
                    cr.show_text(&label).ok();

                    // Protected indicator: small lock symbol at top-right
                    if is_protected {
                        let lx = left + NODE_WIDTH - 14.0;
                        let ly = top + 3.0;
                        cr.set_source_rgba(0.85, 0.7, 0.2, 0.9);
                        cr.set_font_size(11.0);
                        cr.move_to(lx, ly + 10.0);
                        cr.show_text("\u{1f512}").ok(); // 🔒
                    }
                }
            });
        }

        // Hover tooltip
        {
            let layout_tt = layout.clone();
            let dep_tree_tt = dep_tree.clone();
            let zoom_tt = zoom.clone();
            drawing_area.set_has_tooltip(true);
            drawing_area.connect_query_tooltip(move |_da, x, y, _keyboard, tooltip| {
                let z = zoom_tt.get();
                let layout_ref = layout_tt.borrow();
                let Some(ref l) = *layout_ref else {
                    return false;
                };
                let Some(node_id) = l.hit_test(x as f64 / z, y as f64 / z) else {
                    return false;
                };

                let tree_ref = dep_tree_tt.borrow();
                let Some(ref tree) = *tree_ref else {
                    return false;
                };
                let Some(pkg) = tree.get(node_id) else {
                    return false;
                };

                let vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
                vbox.set_margin_start(4);
                vbox.set_margin_end(4);
                vbox.set_margin_top(2);
                vbox.set_margin_bottom(2);

                let display = if pkg.display_name.is_empty() {
                    &pkg.name
                } else {
                    &pkg.display_name
                };
                let title = gtk::Label::new(Some(&format!("{display} {}", pkg.version)));
                title.set_halign(gtk::Align::Start);
                title.add_css_class("heading");
                vbox.append(&title);

                if !pkg.display_name.is_empty() && pkg.display_name != pkg.name {
                    let tech = gtk::Label::new(Some(&pkg.name));
                    tech.add_css_class("dim-label");
                    tech.set_halign(gtk::Align::Start);
                    vbox.append(&tech);
                }

                let info = format!(
                    "{} | {} | Deps: {} | Required by: {}",
                    pkg.source.label(),
                    bytesize::ByteSize(pkg.installed_size),
                    pkg.depends.len(),
                    pkg.required_by.len(),
                );
                let info_label = gtk::Label::new(Some(&info));
                info_label.set_halign(gtk::Align::Start);
                info_label.add_css_class("dim-label");
                vbox.append(&info_label);

                if let Some(ts) = pkg.install_date {
                    let date_str = format_timestamp(ts);
                    let date_label = gtk::Label::new(Some(&format!("Installed: {date_str}")));
                    date_label.set_halign(gtk::Align::Start);
                    date_label.add_css_class("dim-label");
                    vbox.append(&date_label);
                }

                if !pkg.description.is_empty() {
                    let first_line = pkg.description.lines().next().unwrap_or("");
                    let desc = gtk::Label::new(Some(first_line));
                    desc.set_halign(gtk::Align::Start);
                    desc.set_wrap(true);
                    desc.set_max_width_chars(50);
                    vbox.append(&desc);
                }

                tooltip.set_custom(Some(&vbox));
                true
            });
        }

        // Mouse motion controller for hover highlight
        let motion_controller = gtk::EventControllerMotion::new();
        {
            let hovered = hovered_node.clone();
            let layout = layout.clone();
            let da = drawing_area.clone();
            let zoom_hover = zoom.clone();

            motion_controller.connect_motion(move |_ctrl, x, y| {
                let z = zoom_hover.get();
                let layout_ref = layout.borrow();
                let new_hover = if let Some(ref l) = *layout_ref {
                    l.hit_test(x / z, y / z).map(String::from)
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

        // Right-click to show popover with package details
        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        {
            let layout = layout.clone();
            let dep_tree_rc = dep_tree.clone();
            let da = drawing_area.clone();
            let active_popover: Rc<RefCell<Option<gtk::Popover>>> = Rc::new(RefCell::new(None));
            let zoom_rc = zoom.clone();

            right_click.connect_released(move |_gesture, _n, x, y| {
                let z = zoom_rc.get();
                if let Some(ref p) = *active_popover.borrow() {
                    p.popdown();
                    p.unparent();
                }
                *active_popover.borrow_mut() = None;

                let layout_ref = layout.borrow();
                let Some(ref l) = *layout_ref else { return };
                let Some(node_id) = l.hit_test(x / z, y / z) else {
                    return;
                };

                if let Some((left, top, w, _h)) = l.node_rect(node_id) {
                    let tree_ref = dep_tree_rc.borrow();
                    if let Some(ref tree) = *tree_ref {
                        if let Some(pkg) = tree.get(node_id) {
                            let p =
                                graph_node::create_popover(&da, pkg, tree, left + w / 2.0, top);
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
            let explicit_ids = explicit_ids.clone();
            let orphan_click = orphan_preview.clone();
            let da = drawing_area.clone();
            let on_changed = on_selection_changed.clone();
            let zoom_click = zoom.clone();

            click_controller.connect_released(move |_gesture, _n_press, x, y| {
                let z = zoom_click.get();
                let layout_ref = layout.borrow();
                let Some(ref l) = *layout_ref else { return };

                if let Some(node_id) = l.hit_test(x / z, y / z) {
                    let node_id = node_id.to_string();

                    // Don't allow selecting protected packages
                    if let Some(ref tree) = *dep_tree.borrow() {
                        if let Some(pkg) = tree.get(&node_id) {
                            if pkg.is_protected {
                                return;
                            }
                        }
                    }

                    // Only explicitly selected apps go in the removal set.
                    // pacman -Rns handles orphaned dependency cleanup itself.
                    let mut sel = selected_ids.borrow_mut();
                    let mut explicit = explicit_ids.borrow_mut();

                    if sel.contains(&node_id) {
                        sel.remove(&node_id);
                        explicit.remove(&node_id);
                    } else {
                        sel.insert(node_id.clone());
                        explicit.insert(node_id);
                    }

                    // Compute orphan preview (visual only, not part of selection)
                    let mut orphans = orphan_click.borrow_mut();
                    orphans.clear();
                    if !sel.is_empty() {
                        if let Some(ref tree) = *dep_tree.borrow() {
                            let full_branch = tree.compute_removal_branch(&sel);
                            // Orphans = full branch minus what was explicitly selected
                            for id in &full_branch {
                                if !sel.contains(id) {
                                    orphans.insert(id.clone());
                                }
                            }
                        }
                    }

                    drop(orphans);
                    drop(sel);
                    drop(explicit);
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

        // Ctrl+Scroll to zoom (centered on cursor)
        {
            let zoom_scroll = zoom.clone();
            let layout_zoom = layout.clone();
            let da_zoom = drawing_area.clone();
            let scroll_zoom = scroll.clone();
            let scroll_ctrl = gtk::EventControllerScroll::new(
                gtk::EventControllerScrollFlags::VERTICAL,
            );
            scroll_ctrl.connect_scroll(move |ctrl, _dx, dy| {
                // Only zoom when Ctrl is held
                let state = ctrl.current_event_state();
                if !state.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                    return gtk::glib::Propagation::Proceed;
                }

                let old_z = zoom_scroll.get();
                let factor = if dy < 0.0 { 1.1 } else { 1.0 / 1.1 };
                let new_z = (old_z * factor).clamp(0.1, 5.0);

                // Get mouse position in widget coords from scroll adjustments
                let h = scroll_zoom.hadjustment();
                let v = scroll_zoom.vadjustment();
                // Mouse position relative to widget — approximate with viewport center
                // since EventControllerScroll doesn't provide coords directly
                let mouse_x = h.page_size() / 2.0;
                let mouse_y = v.page_size() / 2.0;

                // Canvas point under cursor before zoom
                let canvas_x = (h.value() + mouse_x) / old_z;
                let canvas_y = (v.value() + mouse_y) / old_z;

                zoom_scroll.set(new_z);

                // Update content size
                let layout_ref = layout_zoom.borrow();
                if let Some(ref l) = *layout_ref {
                    da_zoom.set_content_width((l.canvas_width * new_z) as i32);
                    da_zoom.set_content_height((l.canvas_height * new_z) as i32);
                }
                drop(layout_ref);

                // Adjust scroll so the same canvas point stays under cursor
                let new_scroll_x = canvas_x * new_z - mouse_x;
                let new_scroll_y = canvas_y * new_z - mouse_y;
                h.set_value(new_scroll_x);
                v.set_value(new_scroll_y);

                da_zoom.queue_draw();
                gtk::glib::Propagation::Stop
            });
            drawing_area.add_controller(scroll_ctrl);
        }

        // Middle-click drag to pan — raw event handling for flicker-free scrolling.
        // EventControllerLegacy gives us surface-relative coordinates via event.position()
        // which don't shift when we adjust scrollbars, avoiding feedback loops.
        {
            let scroll_pan = scroll.clone();
            let is_panning: Rc<Cell<bool>> = Rc::new(Cell::new(false));
            let pan_origin_x: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));
            let pan_origin_y: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));
            let pan_start_h: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));
            let pan_start_v: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));

            let pan_ctrl = gtk::EventControllerLegacy::new();

            {
                let sp = scroll_pan;
                let panning = is_panning;
                let ox = pan_origin_x;
                let oy = pan_origin_y;
                let sh = pan_start_h;
                let sv = pan_start_v;

                pan_ctrl.connect_event(move |_ctrl, event| {
                    let etype = event.event_type();

                    if etype == gtk::gdk::EventType::ButtonPress {
                        if let Some(btn) = event.downcast_ref::<gtk::gdk::ButtonEvent>() {
                            if btn.button() == 2 {
                                let Some((x, y)) = event.position() else {
                                    return gtk::glib::Propagation::Proceed;
                                };
                                ox.set(x);
                                oy.set(y);
                                sh.set(sp.hadjustment().value());
                                sv.set(sp.vadjustment().value());
                                panning.set(true);
                                return gtk::glib::Propagation::Stop;
                            }
                        }
                    } else if etype == gtk::gdk::EventType::ButtonRelease && panning.get() {
                        if let Some(btn) = event.downcast_ref::<gtk::gdk::ButtonEvent>() {
                            if btn.button() == 2 {
                                panning.set(false);
                                return gtk::glib::Propagation::Stop;
                            }
                        }
                    } else if etype == gtk::gdk::EventType::MotionNotify && panning.get() {
                        let Some((x, y)) = event.position() else {
                            return gtk::glib::Propagation::Proceed;
                        };
                        sp.hadjustment().set_value(sh.get() - (x - ox.get()));
                        sp.vadjustment().set_value(sv.get() - (y - oy.get()));
                        return gtk::glib::Propagation::Stop;
                    }

                    gtk::glib::Propagation::Proceed
                });
            }

            scroll.add_controller(pan_ctrl);
        }

        // --- Toolbar row 1: dropdowns ---
        let toolbar_row1 = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        toolbar_row1.set_margin_start(8);
        toolbar_row1.set_margin_end(8);
        toolbar_row1.set_margin_top(4);

        // Filter dropdown
        let filter_label = gtk::Label::new(Some("Filter:"));
        filter_label.add_css_class("dim-label");
        toolbar_row1.append(&filter_label);

        let filter_model =
            gtk::StringList::new(&["Desktop Apps", "All Explicit", "Pacman", "AUR"]);
        let filter_dropdown = gtk::DropDown::new(Some(filter_model), gtk::Expression::NONE);
        filter_dropdown.set_selected(1); // "All Explicit" — show all explicitly installed packages
        toolbar_row1.append(&filter_dropdown);

        // Sort dropdown
        let sort_label = gtk::Label::new(Some("Sort:"));
        sort_label.add_css_class("dim-label");
        toolbar_row1.append(&sort_label);

        let sort_model = gtk::StringList::new(&[
            "A-Z",
            "Install Date",
            "Size",
            "Dependency Count",
            "Category",
        ]);
        let sort_dropdown = gtk::DropDown::new(Some(sort_model), gtk::Expression::NONE);
        sort_dropdown.set_selected(0);
        toolbar_row1.append(&sort_dropdown);

        // Category filter dropdown
        let cat_label = gtk::Label::new(Some("Category:"));
        cat_label.add_css_class("dim-label");
        toolbar_row1.append(&cat_label);

        let mut cat_items: Vec<&str> = vec!["All"];
        cat_items.extend_from_slice(CATEGORY_LABELS);
        let cat_model = gtk::StringList::new(&cat_items);
        let cat_dropdown = gtk::DropDown::new(Some(cat_model), gtk::Expression::NONE);
        cat_dropdown.set_selected(0);
        toolbar_row1.append(&cat_dropdown);

        // Count label (right-aligned in toolbar row 1)
        let count_label = gtk::Label::new(Some("0 packages"));
        count_label.add_css_class("dim-label");
        count_label.set_hexpand(true);
        count_label.set_halign(gtk::Align::End);
        toolbar_row1.append(&count_label);

        // --- Toolbar row 2: sliders ---
        let toolbar_row2 = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        toolbar_row2.set_margin_start(8);
        toolbar_row2.set_margin_end(8);
        toolbar_row2.set_margin_bottom(4);

        // Depth slider
        let depth_text_label = gtk::Label::new(Some("Depth: 1"));
        depth_text_label.add_css_class("dim-label");
        depth_text_label.set_width_chars(16);
        depth_text_label.set_xalign(0.0);
        toolbar_row2.append(&depth_text_label);

        let depth_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 10.0, 1.0);
        depth_scale.set_value(1.0);
        depth_scale.set_hexpand(true);
        depth_scale.set_draw_value(false);
        toolbar_row2.append(&depth_scale);

        // Max Shared slider (filter deps by reverse-dep count)
        let shared_text_label = gtk::Label::new(Some("Max Shared: all"));
        shared_text_label.add_css_class("dim-label");
        shared_text_label.set_width_chars(18);
        shared_text_label.set_xalign(0.0);
        toolbar_row2.append(&shared_text_label);

        let shared_scale =
            gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 50.0, 1.0);
        shared_scale.set_value(50.0);
        shared_scale.set_hexpand(true);
        shared_scale.set_draw_value(false);
        toolbar_row2.append(&shared_scale);

        // Helper: trigger rebuild with current state
        macro_rules! wire_rebuild {
            ($( $clone_name:ident : $src:expr ),+ ; $body:expr) => {{
                $( let $clone_name = $src.clone(); )+
                let layout_rc = layout.clone();
                let dep_tree_rc = dep_tree.clone();
                let da = drawing_area.clone();
                let filter_c = filter.clone();
                let sort_c = sort.clone();
                let depth_c = depth.clone();
                let fans_c = max_dep_fans.clone();
                let cat_c = category_idx.clone();
                let zoom_c = zoom.clone();
                let cl = count_label.clone();
                #[allow(clippy::redundant_closure_call)]
                ($body)(Box::new(move || {
                    let cat_f = if cat_c.get() == 0 {
                        None
                    } else {
                        Some(CATEGORY_LABELS[cat_c.get() - 1])
                    };
                    rebuild_layout_inner(
                        &dep_tree_rc,
                        &layout_rc,
                        filter_c.get(),
                        sort_c.get(),
                        depth_c.get(),
                        fans_c.get(),
                        cat_f,
                        &da,
                        zoom_c.get(),
                    );
                    let n = layout_rc.borrow().as_ref().map(|l| l.node_positions.len()).unwrap_or(0);
                    cl.set_label(&format!("{n} packages"));
                }))
            }};
        }

        // Wire filter dropdown
        wire_rebuild!(filter_rc: filter; |rebuild: Box<dyn Fn() + 'static>| {
            let filter_rc2 = filter_rc.clone();
            filter_dropdown.connect_selected_notify(move |dd| {
                let f = match dd.selected() {
                    0 => GraphFilter::DesktopApps,
                    1 => GraphFilter::All,
                    2 => GraphFilter::Pacman,
                    3 => GraphFilter::Aur,
                    _ => GraphFilter::DesktopApps,
                };
                filter_rc2.set(f);
                rebuild();
            });
        });

        // Wire sort dropdown
        wire_rebuild!(sort_rc: sort; |rebuild: Box<dyn Fn() + 'static>| {
            let sort_rc2 = sort_rc.clone();
            sort_dropdown.connect_selected_notify(move |dd| {
                let s = match dd.selected() {
                    0 => GraphSort::Alphabetical,
                    1 => GraphSort::InstallDate,
                    2 => GraphSort::InstalledSize,
                    3 => GraphSort::DependencyCount,
                    4 => GraphSort::Category,
                    _ => GraphSort::Alphabetical,
                };
                sort_rc2.set(s);
                rebuild();
            });
        });

        // Wire category dropdown
        wire_rebuild!(cat_rc: category_idx; |rebuild: Box<dyn Fn() + 'static>| {
            let cat_rc2 = cat_rc.clone();
            cat_dropdown.connect_selected_notify(move |dd| {
                cat_rc2.set(dd.selected() as usize);
                rebuild();
            });
        });

        // Wire depth slider
        {
            let depth_rc = depth.clone();
            let filter_rc = filter.clone();
            let sort_rc = sort.clone();
            let fans_rc = max_dep_fans.clone();
            let cat_rc = category_idx.clone();
            let zoom_rc = zoom.clone();
            let layout_rc = layout.clone();
            let dep_tree_rc = dep_tree.clone();
            let da = drawing_area.clone();
            let dtl = depth_text_label.clone();
            let cl = count_label.clone();
            depth_scale.connect_value_changed(move |scale| {
                let d = scale.value() as usize;
                depth_rc.set(d);
                let label_text = match d {
                    0 => "Depth: 0 (apps only)".to_string(),
                    10 => "Depth: 10 (all)".to_string(),
                    _ => format!("Depth: {d}"),
                };
                dtl.set_label(&label_text);
                let cat_f = if cat_rc.get() == 0 {
                    None
                } else {
                    Some(CATEGORY_LABELS[cat_rc.get() - 1])
                };
                rebuild_layout_inner(
                    &dep_tree_rc, &layout_rc, filter_rc.get(), sort_rc.get(),
                    d, fans_rc.get(), cat_f, &da, zoom_rc.get(),
                );
                let n = layout_rc.borrow().as_ref().map(|l| l.node_positions.len()).unwrap_or(0);
                cl.set_label(&format!("{n} packages"));
            });
        }

        // Wire max shared slider
        {
            let fans_rc = max_dep_fans.clone();
            let filter_rc = filter.clone();
            let sort_rc = sort.clone();
            let depth_rc = depth.clone();
            let cat_rc = category_idx.clone();
            let zoom_rc = zoom.clone();
            let layout_rc = layout.clone();
            let dep_tree_rc = dep_tree.clone();
            let da = drawing_area.clone();
            let stl = shared_text_label.clone();
            let cl = count_label.clone();
            shared_scale.connect_value_changed(move |scale| {
                let v = scale.value() as usize;
                let effective = if v >= 50 { usize::MAX } else { v };
                fans_rc.set(effective);
                let label_text = if v >= 50 {
                    "Max Shared: all".to_string()
                } else {
                    format!("Max Shared: {v}")
                };
                stl.set_label(&label_text);
                let cat_f = if cat_rc.get() == 0 {
                    None
                } else {
                    Some(CATEGORY_LABELS[cat_rc.get() - 1])
                };
                rebuild_layout_inner(
                    &dep_tree_rc, &layout_rc, filter_rc.get(), sort_rc.get(),
                    depth_rc.get(), effective, cat_f, &da, zoom_rc.get(),
                );
                let n = layout_rc.borrow().as_ref().map(|l| l.node_positions.len()).unwrap_or(0);
                cl.set_label(&format!("{n} packages"));
            });
        }

        // Outer box: two toolbar rows + scroll
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        outer.append(&toolbar_row1);
        outer.append(&toolbar_row2);
        outer.append(&scroll);

        Self {
            widget: outer,
            drawing_area,
            layout,
            hovered_node,
            search_matches,
            search_active,
            orphan_preview,
            dep_tree,
            filter,
            sort,
            depth,
            max_dep_fans,
            category_idx,
            zoom,
            scroll_window: scroll,
            count_label,
        }
    }

    /// Rebuild the graph layout from the current dep_tree and filter/sort/depth state.
    pub fn rebuild_layout(&self) {
        let cat_f = if self.category_idx.get() == 0 {
            None
        } else {
            Some(CATEGORY_LABELS[self.category_idx.get() - 1])
        };
        rebuild_layout_inner(
            &self.dep_tree,
            &self.layout,
            self.filter.get(),
            self.sort.get(),
            self.depth.get(),
            self.max_dep_fans.get(),
            cat_f,
            &self.drawing_area,
            self.zoom.get(),
        );
        self.update_count_label();
    }

    fn update_count_label(&self) {
        let layout_ref = self.layout.borrow();
        let count = layout_ref.as_ref().map(|l| l.node_positions.len()).unwrap_or(0);
        self.count_label.set_label(&format!("{count} packages"));
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

/// Shared rebuild logic used by toolbar callbacks and `rebuild_layout()`.
fn rebuild_layout_inner(
    dep_tree: &Rc<RefCell<Option<DependencyTree>>>,
    layout: &Rc<RefCell<Option<GraphLayout>>>,
    filter: GraphFilter,
    sort: GraphSort,
    depth: usize,
    max_dep_fans: usize,
    category_filter: Option<&str>,
    da: &gtk::DrawingArea,
    zoom: f64,
) {
    let tree_ref = dep_tree.borrow();
    let Some(ref tree) = *tree_ref else { return };

    let new_layout = GraphLayout::build(tree, filter, sort, depth, max_dep_fans, category_filter);
    da.set_content_width((new_layout.canvas_width * zoom) as i32);
    da.set_content_height((new_layout.canvas_height * zoom) as i32);
    *layout.borrow_mut() = Some(new_layout);
    da.queue_draw();
}

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

/// Load an icon from the icon theme as a Cairo ImageSurface for painting.
fn load_icon_surface(
    theme: &gtk::IconTheme,
    icon_name: &str,
    size: i32,
) -> Option<gtk::cairo::ImageSurface> {
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
    let file = icon_paintable.file()?;
    let path = file.path()?;
    let texture = gtk::gdk::Texture::from_file(&gio::File::for_path(&path)).ok()?;

    let w = texture.width();
    let h = texture.height();
    // Cairo ARGB32 format matches GDK's default B8G8R8A8_PREMULTIPLIED on little-endian
    let mut surface =
        gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w, h).ok()?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data().ok()?;
        texture.download(&mut data, stride);
    }
    surface.mark_dirty();
    Some(surface)
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

/// Format a Unix timestamp to "YYYY-MM-DD" for display.
fn format_timestamp(ts: i64) -> String {
    let is_leap = |y: i64| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days_in_month = |y: i64, m: i64| -> i64 {
        match m {
            1 => 31,
            2 => if is_leap(y) { 29 } else { 28 },
            3 => 31,
            4 => 30,
            5 => 31,
            6 => 30,
            7 => 31,
            8 => 31,
            9 => 30,
            10 => 31,
            11 => 30,
            12 => 31,
            _ => 30,
        }
    };

    let mut remaining_days = ts / 86400;
    let mut year = 1970;
    loop {
        let days_this_year = if is_leap(year) { 366 } else { 365 };
        if remaining_days < days_this_year {
            break;
        }
        remaining_days -= days_this_year;
        year += 1;
    }

    let mut month = 1;
    loop {
        let dim = days_in_month(year, month);
        if remaining_days < dim {
            break;
        }
        remaining_days -= dim;
        month += 1;
    }
    let day = remaining_days + 1;

    format!("{year:04}-{month:02}-{day:02}")
}
