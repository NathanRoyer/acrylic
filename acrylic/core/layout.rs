use super::visual::{
    Pixels, Ratio, Axis::{self, Horizontal, Vertical},
    LayoutMode::*, Size, Position, SignedPixels,
};
use super::node::{Node, NodeTree, NodeKey};
use super::event::{Event};
use super::app::Application;
use super::for_each_child;
use crate::Error;

pub fn hit_test(tree: &mut NodeTree, root: NodeKey, p: Position) -> NodeKey {
    let mut current = root;

    'outer: loop {
        for_each_child!(tree, current, child, {
            let node_pos = tree[child].position;
            let node_max = node_pos.add_size(tree[child].size);
            let x_in_range = node_pos.x <= p.x && node_max.x > p.x;
            let y_in_range = node_pos.y <= p.y && node_max.y > p.y;
            if x_in_range && y_in_range {
                current = child;
                continue 'outer;
            }
        });
        break;
    }

    current
}

pub fn get_scroll(app: &Application, container: NodeKey) -> (Axis, Option<SignedPixels>, Option<Pixels>) {
    let node = &app.view[container];
    let axis = node.layout_config.get_length_axis();
    let p = node.position.add_size(node.margin.top_left).get_for_axis(axis);
    let content_capacity = node.size.get_for_axis(axis);
    let mut scroll = None;

    let mut cursor = Cursor::new(&app.view[container], node.position);
    for_each_child!(app.view, container, child, {
        if let None = scroll {
            scroll = Some(p - app.view[child].position.get_for_axis(axis));
        }

        cursor.advance(&app.view[child]);
    });

    let content_length = cursor.finish();
    let max_scroll = content_length.checked_sub(content_capacity);
    (axis, scroll.filter(|v| !v.is_zero()), max_scroll.filter(|v| !v.is_zero()))
}

pub fn scroll(app: &mut Application, container: NodeKey, axis: Axis, diff: SignedPixels) {
    for_each_child!(app.view, container, child, {
        scroll(app, child, axis, diff);
        app.view[child].position.add_to_axis(axis, diff);
    });
}

pub fn compute_layout(app: &mut Application, root: NodeKey) -> Result<(), Error> {
    app.view[root].layout_config.set_size_found(true);
    let axis = app.view[root].layout_config.get_content_axis();
    let comp_axis = axis.complement();
    let mut cross = app.view[root].size.get_for_axis(comp_axis);
    cross = cross.checked_sub(app.view[root].margin.total_on(comp_axis)).unwrap_or(Pixels::ZERO);
    compute_children_sizes(&mut app.view, root, cross);
    compute_remaining_children_sizes(&mut app.view, root, cross);
    compute_positions(app, root, Position::default())
}

impl Node {
    #[inline(always)]
    pub fn set_size(&mut self, size: Size) {
        self.layout_config.set_size_found(true);
        if self.size != size {
            self.layout_config.set_resized(true);
            self.size = size;
        }
    }
}

fn compute_positions(app: &mut Application, key: NodeKey, top_left: Position) -> Result<(), Error> {
    let mut cursor = Cursor::new(&app.view[key], top_left);
    for_each_child!(app.view, key, child, {
        let size_found = app.view[child].layout_config.get_size_found();
        if !size_found && app.view[child].size != Size::zero() {
            app.view[child].set_size(Size::zero());
            app.view[child].layout_config.set_resized(true);
        } else {
            app.view[child].layout_config.set_size_found(false);
        }

        let position = cursor.advance(&app.view[child]);
        let moved = app.view[child].position != position;
        let resized = app.view[child].layout_config.get_resized();

        app.view[child].position = position;
        compute_positions(app, child, position)?;

        if moved {
            app.view[child].layout_config.set_dirty(true);
        }

        if resized {
            app.view[child].layout_config.set_resized(false);
            app.handle(child, Event::Resized { node_key: child })?;
        }
    });

    Ok(())
}

fn handle_children(tree: &mut NodeTree, container: NodeKey) {
    let axis = tree[container].layout_config.get_content_axis();
    let cross = tree[container].size.get_for_axis(axis.complement());
    if let Some(cross) = adjust_cross(&tree[container], cross) {
        compute_children_sizes(tree, container, cross);
        compute_remaining_children_sizes(tree, container, cross);
    }
}

fn get_children_length_on_cont_axis(tree: &mut NodeTree, container: NodeKey) -> Pixels {
    let axis = tree[container].layout_config.get_content_axis();
    let gap = tree[container].layout_config.get_content_gap();
    let mut length = Pixels::ZERO;

    for_each_child!(tree, container, child, {
        length += tree[child].size.get_for_axis(axis) + gap;
    });

    // remove last gap if there are children
    if length > Pixels::ZERO {
        length -= gap;
    }

    length + tree[container].margin.total_on(axis)
}

fn compute_children_sizes(tree: &mut NodeTree, container: NodeKey, cross: Pixels) {
    let axis = tree[container].layout_config.get_content_axis();
    for_each_child!(tree, container, child, {
        match tree[child].layout_config.get_layout_mode() {
            WrapContent => compute_wrapper_size(tree, axis, child, Some(cross)),
            Fixed(l) => compute_fixed_size(tree, axis, child, Some(cross), l),
            Chunks(r) => compute_chunks_size(tree, axis, child, cross, r),
            AspectRatio(r) => {
                let comp_len = match axis {
                    Horizontal => cross.checked_mul(r.to_num()),
                    Vertical => cross.checked_div(r.to_num()),
                };

                comp_len.and_then(|r| r.checked_round()).map(|length| {
                    let size = match axis {
                        Horizontal => Size::new(length, cross),
                        Vertical => Size::new(cross, length),
                    };

                    tree[child].set_size(size);
                    handle_children(tree, child);
                })
            }
            Remaining(_) | Unset => None,
        };
    });
}

fn compute_remaining_children_sizes(tree: &mut NodeTree, container: NodeKey, cross: Pixels) {
    let layout_config = tree[container].layout_config;
    if !layout_config.get_size_found() {
        return;
    }

    let axis = layout_config.get_content_axis();
    let gap = layout_config.get_content_gap();
    let mut quota_sum = Ratio::ZERO;
    let mut used = Pixels::ZERO;
    let mut seen = false;

    for_each_child!(tree, container, child, {
        if let Remaining(_) = tree[child].layout_config.get_layout_mode() {
            seen = true;
            break;
        }
    });

    if !seen {
        return;
    }

    let available = if let Chunks(_) = layout_config.get_layout_mode() {
        Pixels::ZERO
    } else {
        for_each_child!(tree, container, child, {
            if let Remaining(q) = tree[child].layout_config.get_layout_mode() {
                quota_sum += q;
                used += gap;
            } else {
                used += tree[child].size.get_for_axis(axis) + gap;
            }
        });

        // remove last gap if there are no children
        if used > Pixels::ZERO {
            used -= gap;
        }

        used += tree[container].margin.total_on(axis);
        let total = tree[container].size.get_for_axis(axis);
        total.checked_sub(used).unwrap_or(Pixels::ZERO)
    };

    for_each_child!(tree, container, child, {
        if let Remaining(q) = tree[child].layout_config.get_layout_mode() {
            let fraction = q.checked_div(quota_sum.to_num()).unwrap_or(Default::default());
            let length = available.saturating_mul(fraction.to_num());

            let size = match axis {
                Horizontal => Size::new(length, cross),
                Vertical => Size::new(cross, length),
            };

            tree[child].set_size(size);
            handle_children(tree, child);
        }
    });
}

fn compute_wrapper_size(
    tree: &mut NodeTree,
    cont_axis: Axis,
    wrapper: NodeKey,
    mut cross: Option<Pixels>,
) -> Option<()> {
    let wrapper_axis = tree[wrapper].layout_config.get_content_axis();
    let mut length = Pixels::ZERO;

    if wrapper_axis != cont_axis {
        length = cross.unwrap_or(Pixels::ZERO);
        // pass 1 for cross length
        cross = get_max_length_on(tree, cont_axis, wrapper, None);
    }

    let cross = cross?;
    let apparent_cross = adjust_cross(&tree[wrapper], cross)?;

    // pass 2
    compute_children_sizes(tree, wrapper, apparent_cross);
    if length == Pixels::ZERO {
        length = get_children_length_on_cont_axis(tree, wrapper);
    }

    let size = match cont_axis {
        Horizontal => Size::new(cross, length),
        Vertical => Size::new(length, cross),
    };

    tree[wrapper].set_size(size);
    compute_remaining_children_sizes(tree, wrapper, apparent_cross);

    Some(())
}

fn compute_fixed_size(
    tree: &mut NodeTree,
    cont_axis: Axis,
    fixed: NodeKey,
    mut cross: Option<Pixels>,
    length: Pixels,
) -> Option<()> {
    let axis = tree[fixed].layout_config.get_content_axis();
    let has_children = tree.first_child(fixed).is_some();

    // if this is a non-empty container, the cross-length
    // can be computed from the children for diff-axis
    // configurations
    if has_children {
        let same_axis = axis == cont_axis;
        let children_cross = match same_axis {
            true => cross?,
            false => length,
        };

        if let Some(children_cross) = adjust_cross(&tree[fixed], children_cross) {
            compute_children_sizes(tree, fixed, children_cross);
        }

        if cross.is_none() && !same_axis {
            cross = Some(get_children_length_on_cont_axis(tree, fixed));
        }
    }

    let cross = cross?;
    let size = match cont_axis {
        Horizontal => Size::new(length, cross),
        Vertical => Size::new(cross, length),
    };

    tree[fixed].set_size(size);

    if has_children {
        let cross = size.get_for_axis(axis.complement());
        if let Some(cross) = adjust_cross(&tree[fixed], cross) {
            compute_remaining_children_sizes(tree, fixed, cross);
        }
    }

    Some(())
}

fn compute_chunks_size(
    tree: &mut NodeTree,
    cont_axis: Axis,
    this: NodeKey,
    cross: Pixels,
    row: Pixels,
) -> Option<()> {
    let this_axis = tree[this].layout_config.get_content_axis();
    let gap = tree[this].layout_config.get_content_gap();

    if this_axis == cont_axis {
        return compute_wrapper_size(tree, cont_axis, this, Some(cross));
    }

    let cross = adjust_cross(&tree[this], cross)?;
    compute_children_sizes(tree, this, row);
    let mut chunks = 1;
    let mut interline_gap_sum = Pixels::ZERO;
    let mut chunk_length = Pixels::ZERO;

    for_each_child!(tree, this, child, {
        let child_length = tree[child].size.get_for_axis(this_axis);
        let new_chunk_length = chunk_length + gap + child_length;
        if new_chunk_length > cross {
            // carriage return
            interline_gap_sum += gap;
            chunks += 1;
            chunk_length = child_length;
        } else {
            chunk_length = new_chunk_length;
        }
    });

    let length = row * chunks + interline_gap_sum + tree[this].margin.total_on(cont_axis);
    let size = match cont_axis {
        Horizontal => Size::new(length, cross),
        Vertical => Size::new(cross, length),
    };

    tree[this].set_size(size);
    compute_remaining_children_sizes(tree, this, row);

    Some(())
}

fn get_max_length_on(
    tree: &mut NodeTree,
    wanted_axis: Axis,
    cont: NodeKey,
    cross: Option<Pixels>,
) -> Option<Pixels> {
    // wanted_axis = horizontal
    // wrapper_axis = vertical
    let cont_axis = tree[cont].layout_config.get_content_axis();

    // none
    let cross = match cross {
        Some(c) => Some(adjust_cross(&tree[cont], c)?),
        None => None,
    };

    let mut max = None;
    for_each_child!(tree, cont, child, {
        let child_axis = tree.first_child(child).map(|_| {
            tree[child].layout_config.get_content_axis()
        });
        let same_axis = Some(cont_axis) == child_axis;

        let candidate = match tree[child].layout_config.get_layout_mode() {
            WrapContent => match (Some(wanted_axis) == child_axis, same_axis) {
                (true, _) => {
                    compute_wrapper_size(tree, cont_axis, child, cross).map(|_| {
                        tree[child].size.get_for_axis(wanted_axis)
                    })
                },
                (false, true) => get_max_length_on(tree, wanted_axis, child, cross),
                (false, false) => get_max_length_on(tree, wanted_axis, child, None),
            },
            Fixed(l) => match (cont_axis == wanted_axis, Some(wanted_axis) == child_axis, same_axis) {
                (true,  _,    _) => Some(l),
                (false, true, _) => {
                    compute_fixed_size(tree, cont_axis, child, cross, l).map(|_| {
                        tree[child].size.get_for_axis(wanted_axis)
                    })
                },
                (false, false, true) => get_max_length_on(tree, wanted_axis, child, cross),
                (false, false, false) => get_max_length_on(tree, wanted_axis, child, Some(l)),
            },
            Chunks(row) => if same_axis {
                // treat Chunks in same-axis config as WrapContent
                if Some(wanted_axis) == child_axis {
                    compute_wrapper_size(tree, cont_axis, child, cross).map(|_| {
                        tree[child].size.get_for_axis(wanted_axis)
                    })
                } else {
                    get_max_length_on(tree, wanted_axis, child, cross)
                }
            } else if cont_axis != wanted_axis {
                cross.and_then(|cross| compute_chunks_size(tree, cont_axis, child, cross, row)).map(|_| {
                    tree[child].size.get_for_axis(wanted_axis)
                })
            } else {
                None
            },
            AspectRatio(r) if cont_axis != wanted_axis => {
                cross.and_then(|cross| match cont_axis {
                    Horizontal => cross.checked_mul(r.to_num()),
                    Vertical => cross.checked_div(r.to_num()),
                })
            }
            Remaining(_) => get_max_length_on(tree, wanted_axis, child, None),
            _ => None,
        };

        if let Some(len) = candidate {
            if max.map(|max| max < len).unwrap_or(true) {
                max = candidate;
            }
        }
    });

    max.map(|max| max + tree[cont].margin.total_on(wanted_axis))
}

pub struct Cursor {
    axis: Axis,
    gap: Pixels,
    top_left: Position,
    brand_new: bool,
    content_length: Pixels,
    line_start: Position,
    chunk_length: Pixels,
    max_chunk_length: Pixels,
    row: Option<Pixels>,
}

impl Cursor {
    pub fn new(container: &Node, mut top_left: Position) -> Self {
        top_left = top_left.add_size(container.margin.top_left);

        let axis = container.layout_config.get_content_axis();
        let gap = container.layout_config.get_content_gap();
        let length = container.size.get_for_axis(axis);
        let minus = container.margin.total_on(axis);
        let length = length.checked_sub(minus);
        let max_chunk_length = length.unwrap_or(Pixels::ZERO);

        let (content_length, row) = match container.layout_config.get_layout_mode() {
            Chunks(row) => (row, Some(row)),
            _ => (Pixels::ZERO, None),
        };

        Self {
            axis,
            gap,
            row,
            top_left,
            max_chunk_length,
            line_start: top_left,
            brand_new: true,
            content_length,
            chunk_length: Pixels::ZERO,
        }
    }

    pub fn advance(&mut self, child: &Node) -> Position {
        let child_length = child.size.get_for_axis(self.axis);
        let with_gap = child_length + self.gap;

        if let Some(row) = self.row {
            let new_chunk_length = self.chunk_length + with_gap;
            if new_chunk_length > self.max_chunk_length {
                // chunk overflow -> carriage return
                let row_and_gap = row + self.gap;
                let complement_axis = self.axis.complement();
                self.line_start.add_to_axis(complement_axis, row_and_gap.to_num());
                self.content_length += row_and_gap;
                self.top_left = self.line_start;
                self.chunk_length = child_length;
            } else {
                self.chunk_length = new_chunk_length;
            }
        } else {
            self.content_length += child_length;
            match self.brand_new {
                true => self.brand_new = false,
                false => self.content_length += self.gap,
            }
        }

        let position = self.top_left;
        self.top_left.add_to_axis(self.axis, with_gap.to_num());
        position
    }

    pub fn finish(self) -> Pixels {
        self.content_length
    }
}

fn adjust_cross(cont: &Node, cross: Pixels) -> Option<Pixels> {
    let axis = cont.layout_config.get_content_axis();
    let to_sub = cont.margin.total_on(axis.complement());
    cross.checked_sub(to_sub)
}
