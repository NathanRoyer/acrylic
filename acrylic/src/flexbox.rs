use crate::node::Axis;
use crate::node::Axis::Horizontal;
use crate::node::Axis::Vertical;
use crate::node::LengthPolicy::*;
use crate::node::NodeBox;
use crate::node::Margin;
use crate::round;
use crate::status;
use crate::Point;
use crate::Size;
use crate::Status;

/// This function will update the size and position of each
/// node under `root` in a way that ressembles the CSS Flexible
/// Box Layout aglorithm. For each node it encounters, it lays
/// it out according to the [`LengthPolicy`](`crate::node::LengthPolicy`)
/// setting of the node.
///
/// Note: The [`LengthPolicy`](`crate::node::LengthPolicy`)
/// of the root node has no effect, as its size is set
/// by the platform.
///
/// Prefer setting `app.should_recompute` to true
/// instead of calling this directly, when possible.
pub fn compute_tree(root: &mut NodeBox) -> Status {
    let size = root.get_spot_size();
    if let Some((axis, _)) = root.container() {
        let cross = size.get_for_axis(axis.complement());
        let _ = compute_children_sizes(root, cross);
        let _ = compute_remaining_children_sizes(root, cross);
    }
    Ok(())
}

fn unwrap_child(child: &Option<NodeBox>) -> &NodeBox {
    child.as_ref().expect("fatal: kidnapped node during layout")
}

fn unwrap_child_mut(child: &mut Option<NodeBox>) -> &mut NodeBox {
    child.as_mut().expect("fatal: kidnapped node during layout")
}

fn compute_children_sizes(container: &mut NodeBox, cross: usize) -> Status {
    let (axis, _) = status(container.container())?;
    for child in container.children_mut() {
        let child = unwrap_child_mut(child);
        let result = match child.policy() {
            WrapContent => compute_wrapper_size(axis, child, Some(cross)),
            Fixed(l) => compute_fixed_size(axis, child, Some(cross), l),
            Chunks(r) => compute_chunks_size(axis, child, cross, r),
            AspectRatio(r) => {
                let result = match axis {
                    Horizontal => (cross as f64) * r,
                    Vertical => (cross as f64) / r,
                };
                if result.is_finite() && result >= 0.0 {
                    let length = round!(result, f64, usize);
                    let size = match axis {
                        Horizontal => Size::new(length, cross),
                        Vertical => Size::new(cross, length),
                    };
                    child.set_spot_size(size);
                    if let Some((axis, _)) = child.container() {
                        let cross = size.get_for_axis(axis.complement());
                        if let Some(cross) = adjust_cross(child, cross) {
                            let _ = compute_children_sizes(child, cross);
                            let _ = compute_remaining_children_sizes(child, cross);
                        }
                    }
                    Ok(())
                } else {
                    Err(())
                }
            }
            Remaining(_) => Err(()),
        };
        if let Err(()) = result {
            child.set_spot_size(Size::zero());
            recursively_zero(child);
        }
    }
    Ok(())
}

fn recursively_zero(node: &mut NodeBox) {
    for child in node.children_mut() {
        let child = unwrap_child_mut(child);
        child.set_spot_size(Size::zero());
        recursively_zero(child);
    }
}

fn compute_remaining_children_sizes(container: &mut NodeBox, cross: usize) -> Status {
    let (axis, gap) = status(container.container())?;
    let mut quota_sum = 0f64;
    let mut used = 0;
    for child in container.children() {
        let child = unwrap_child(child);
        if let Remaining(q) = child.policy() {
            quota_sum += q;
            used += gap;
        } else {
            let size = child.get_spot_size();
            used += size.get_for_axis(axis) + gap;
        }
    }
    if used > 0 {
        used -= gap;
    }
    if let Some(margin) = container.margin() {
        used += margin.total_on(axis) as usize;
    }
    let size = container.get_spot_size();
    let total = size.get_for_axis(axis);
    let available = (status(total.checked_sub(used))?) as f64;
    for child in container.children_mut() {
        let child = unwrap_child_mut(child);
        if let Remaining(q) = child.policy() {
            let length = (q * available / quota_sum) as usize;
            let size = match axis {
                Horizontal => Size::new(length, cross),
                Vertical => Size::new(cross, length),
            };
            child.set_spot_size(size);
            if let Some((axis, _)) = child.container() {
                let cross = size.get_for_axis(axis.complement());
                if let Some(cross) = adjust_cross(child, cross) {
                    let _ = compute_children_sizes(child, cross);
                    let _ = compute_remaining_children_sizes(child, cross);
                }
            }
        }
    }
    Ok(())
}

fn compute_wrapper_size(
    cont_axis: Axis,
    wrapper: &mut NodeBox,
    mut cross: Option<usize>,
) -> Status {
    let (wrapper_axis, gap) = status(wrapper.container())?;
    let mut length = 0;
    if wrapper_axis != cont_axis {
        length = cross.unwrap_or(0);
        // pass 1 for cross length
        cross = get_max_length_on(cont_axis, wrapper, None);
    }
    let cross = status(cross)?;
    let apparent_cross = status(adjust_cross(wrapper, cross))?;
    // pass 2
    let _ = compute_children_sizes(wrapper, apparent_cross);
    if length == 0 {
        for child in wrapper.children() {
            let child = unwrap_child(child);
            let size = child.get_spot_size();
            length += size.get_for_axis(wrapper_axis) + gap;
        }
        if length > 0 {
            length -= gap;
        }
        if let Some(margin) = wrapper.margin() {
            length += margin.total_on(cont_axis) as usize;
        }
    }
    let size = match wrapper_axis {
        Horizontal => Size::new(length, cross),
        Vertical => Size::new(cross, length),
    };
    wrapper.set_spot_size(size);
    let _ = compute_remaining_children_sizes(wrapper, apparent_cross);
    Ok(())
}

fn compute_fixed_size(
    cont_axis: Axis,
    fixed: &mut NodeBox,
    mut cross: Option<usize>,
    mut length: usize,
) -> Status {
    let mut same_axis = true;
    if let Some((fixed_axis, gap)) = fixed.container() {
        same_axis = fixed_axis == cont_axis;
        let c_cross = match fixed_axis == cont_axis {
            true => status(cross)?,
            false => length,
        };
        if let Some(c_cross) = adjust_cross(fixed, c_cross) {
            let _ = compute_children_sizes(fixed, c_cross);
        }
        if cross.is_none() && fixed_axis != cont_axis {
            let mut length = 0;
            for child in fixed.children() {
                let child = unwrap_child(child);
                let size = child.get_spot_size();
                length += size.get_for_axis(fixed_axis) + gap;
            }
            if length > 0 {
                length -= gap;
            }
            if let Some(margin) = fixed.margin() {
                length += margin.total_on(fixed_axis) as usize;
            }
            cross = Some(length);
        }
    }
    if same_axis {
        if let Some(margin) = fixed.margin() {
            length += margin.total_on(cont_axis) as usize;
        }
    }
    let cross = status(cross)?;
    let size = match cont_axis {
        Horizontal => Size::new(length, cross),
        Vertical => Size::new(cross, length),
    };
    fixed.set_spot_size(size);
    if let Some((fixed_axis, _)) = fixed.container() {
        let cross = match fixed_axis == cont_axis {
            true => cross,
            false => length,
        };
        if let Some(cross) = adjust_cross(fixed, cross) {
            let _ = compute_remaining_children_sizes(fixed, cross);
        }
    }
    Ok(())
}

fn compute_chunks_size(cont_axis: Axis, this: &mut NodeBox, cross: usize, row: usize) -> Status {
    let (this_axis, gap) = status(this.container())?;
    if this_axis == cont_axis {
        Err(())?
    }
    let cross = status(adjust_cross(this, cross))?;
    compute_children_sizes(this, row)?;
    let mut chunks = 1;
    let mut gap_sum = 0;
    let mut chunk_length = 0;
    for child in this.children() {
        let child = unwrap_child(child);
        let size = child.get_spot_size();
        let child_length = size.get_for_axis(this_axis);
        let new_chunk_length = chunk_length + gap + child_length;
        if new_chunk_length > cross {
            gap_sum += gap;
            chunks += 1;
            chunk_length = child_length;
        } else {
            chunk_length = new_chunk_length;
        }
    }
    let mut length = row * chunks + gap_sum;
    if let Some(margin) = this.margin() {
        length += margin.total_on(cont_axis) as usize;
    }
    let size = match this_axis {
        Horizontal => Size::new(cross, length),
        Vertical => Size::new(length, cross),
    };
    this.set_spot_size(size);
    let _ = compute_remaining_children_sizes(this, row);
    Ok(())
}

fn get_max_length_on(axis: Axis, cont: &mut NodeBox, cross: Option<usize>) -> Option<usize> {
    let (cont_axis, _) = cont.container()?;
    let cross = match cross {
        Some(c) => Some(adjust_cross(cont, c)?),
        None => None,
    };
    let mut max = None;
    for child in cont.children_mut() {
        let child = unwrap_child_mut(child);
        let child_axis = child.container().map(|cont| cont.0);
        let same_axis = Some(cont_axis) == child_axis;
        let candidate = match child.policy() {
            WrapContent => match (Some(axis) == child_axis, same_axis) {
                (true, _) => {
                    if let Ok(_) = compute_wrapper_size(cont_axis, child, cross) {
                        let size = child.get_spot_size();
                        Some(size.get_for_axis(axis))
                    } else {
                        None
                    }
                }
                (false, true) => get_max_length_on(axis, child, cross),
                (false, false) => get_max_length_on(axis, child, None),
            },
            Fixed(l) => match (cont_axis == axis, Some(axis) == child_axis, same_axis) {
                (false, true, _) => {
                    if let Ok(_) = compute_fixed_size(cont_axis, child, cross, l) {
                        let size = child.get_spot_size();
                        Some(size.get_for_axis(axis))
                    } else {
                        None
                    }
                }
                (false, false, true) => get_max_length_on(axis, child, cross),
                (false, false, false) => get_max_length_on(axis, child, Some(l)),
                _ => Some(l),
            },
            Chunks(row) if !same_axis && (cont_axis != axis) => {
                let mut result = None;
                if let Some(cross) = cross {
                    if let Ok(_) = compute_chunks_size(cont_axis, child, cross, row) {
                        let size = child.get_spot_size();
                        result = Some(size.get_for_axis(axis));
                    }
                }
                result
            }
            AspectRatio(r) if cont_axis != axis && cross.is_some() => {
                let result = match cont_axis {
                    Horizontal => (cross? as f64) * r,
                    Vertical => (cross? as f64) / r,
                };
                Some(match result.is_finite() && result >= 0.0 {
                    true => result as usize,
                    false => 0,
                })
            }
            _ => None,
        };
        if let Some(len) = candidate {
            let write = match max {
                Some(max) => max < len,
                None => true,
            };
            if write {
                max = candidate;
            }
        }
    }
    if let (Some(margin), Some(max)) = (cont.margin(), max.as_mut()) {
        *max += margin.total_on(axis) as usize;
    }
    max
}

/*fn compute_children_positions(container: &mut NodeBox) -> Status {
    let (axis, gap) = status(container.container())?;
    let (mut base_cursor, size) = status(container.get_content_spot())?;
    let mut cursor = base_cursor;
    let mut chunk_length = 0;
    let max = size.get_for_axis(axis);
    let mut occupied = 0;
    let container_policy = container.policy();
    for child in container.children_mut() {
        let (_, size) = child.get_spot();
        let child_length = size.get_for_axis(axis);
        if let Chunks(row) = container_policy {
            let new_chunk_length = chunk_length + child_length + gap;
            if new_chunk_length > max {
                base_cursor.add_to_axis(axis.complement(), (row + gap) as isize);
                cursor = base_cursor;
                chunk_length = child_length;
            } else {
                chunk_length = new_chunk_length;
            }
        }
        child.set_spot((cursor, size));
        let _ = compute_children_positions(child);
        let length = child_length + gap;
        occupied += length;
        cursor.add_to_axis(axis, length as isize);
    }
    if occupied > 0 {
        occupied -= gap;
    }
    let overflow = occupied.checked_sub(max).unwrap_or(0);
    let _ = container.set_overflow(overflow);
    Ok(())
}*/

pub struct Cursor {
    pub(crate) axis: Axis,
    pub(crate) gap: usize,
    pub(crate) top_left: Point,
    pub(crate) line_start: Point,
    pub(crate) chunk_length: usize,
    pub(crate) max_chunk_length: usize,
    pub(crate) row: Option<usize>,
}

impl Cursor {
    pub fn advance(&mut self, child: &NodeBox) -> (Point, Size, Option<Margin>) {
        let size = child.get_spot_size();
        let child_length = size.get_for_axis(self.axis);
        let with_gap = child_length + self.gap;
        if let Some(row) = self.row {
            let new_chunk_length = self.chunk_length + with_gap;
            if new_chunk_length > self.max_chunk_length {
                // chunk overflow -> carriage return
                self.line_start.add_to_axis(self.axis.complement(), (row + self.gap) as isize);
                self.top_left = self.line_start;
                self.chunk_length = child_length;
            } else {
                self.chunk_length = new_chunk_length;
            }
        }
        let point = self.top_left;
        self.top_left.add_to_axis(self.axis, with_gap as isize);
        (point, size, child.margin())
    }
}

fn adjust_cross(cont: &mut NodeBox, cross: usize) -> Option<usize> {
    let (axis, _) = cont.container()?;
    if let Some(margin) = cont.margin() {
        let to_sub = margin.total_on(axis.complement());
        if cross > to_sub {
            Some(cross - to_sub)
        } else {
            None
        }
    } else {
        Some(cross)
    }
}
