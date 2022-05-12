use crate::node::Node;
use crate::node::RcNode;
use crate::node::Axis::*;
use crate::node::LengthPolicy::*;
use crate::Point;
use crate::Size;
use crate::lock;

use crate::node::SameAxisContainerOrNone;

use core::ops::DerefMut;

use std::println;
#[cfg(not(feature = "std"))]
use std::print;

/// This function will update the size and position of each
/// node under `root` in a way that ressembles the CSS Flexible
/// Box Layout aglorithm. For each node it encounters, it lays
/// it out according to the [`crate::tree::LengthPolicy`] setting of the node.
///
/// Note: This function must never add properties to nodes, so
/// nodes which do not already have size and position settings
/// (whatever their value) are skipped.
pub fn compute_tree(root: &dyn Node) {
	let (orig_pt, n_size) = root.get_spot();
	let (axis, gap) = root.container().expect("flexbox error: root node is not a container!");
	let mut pt = orig_pt;
	let (m, c) = match axis {
		Horizontal => (n_size.w, n_size.h),
		Vertical   => (n_size.h, n_size.w),
	};
	let mut occupied = 0;
	for i in root.children() {
		if let Some(l) = compute_nodes(i, root, None, Some(c), &mut pt) {
			occupied += l;
		}
		occupied += gap;
		pt.add_to_axis(axis, gap as isize);
	}
	if occupied != 0 {
		occupied -= gap;
	}
	pt = orig_pt;
	let available = m - occupied;
	for i in root.children() {
		compute_nodes(i, root, Some(available), Some(c), &mut pt);
		pt.add_to_axis(axis, gap as isize);
	}
}

fn compute_nodes(node: &RcNode, p: &dyn Node, m: Option<usize>, c: Option<usize>, cursor: &mut Point) -> Option<usize> {
	let mut node = lock(node)?;
	let node = node.deref_mut();
	let length = compute_node(node, p, m, c, cursor);
	if let Some((axis, gap)) = node.container() {
		let (p_axis, _) = p.container().unwrap();
		let backup = *cursor;
		*cursor = node.get_spot().0;
		let n_policy = node.policy();
		let same_axis = axis == p_axis;
		let (m, c) = match n_policy {
			Chunks(r) => match same_axis {
				true => {
					println!("flexbox warning: Chunks policy in same-axis config");
					None?
				},
				false => (c, Some(r)),
			},
			_ => match same_axis {
				true => (m, c),
				false => (c, length),
			},
		};
		let mut occupied = 0;
		let mut remaining = 0;
		for j in node.children() {
			remaining += 1;
			if let Some(l) = compute_nodes(j, node, None, c, cursor) {
				occupied += l;
				remaining -= 1;
			}
			occupied += gap;
			cursor.add_to_axis(axis, gap as isize);
		}
		if occupied != 0 {
			occupied -= gap;
		}
		if remaining > 0 {
		if let Some(total) = m {
			if let Some(available) = total.checked_sub(occupied) {
				*cursor = node.get_spot().0;
				for j in node.children() {
					compute_nodes(j, node, Some(available), c, cursor);
					cursor.add_to_axis(axis, gap as isize);
				}
			}
		}
		}
		*cursor = backup;
	}
	length
}

fn compute_node(node: &mut dyn Node, p: &dyn Node, m: Option<usize>, c: Option<usize>, cursor: &mut Point) -> Option<usize> {
	let mut children_cursor = *cursor;
	let n_policy = node.policy();
	let n_container  = node.container();
	let p_container = p.container();
	// println!("{} → {:?}, {:?}, {:?}", i, n_policy, m, c);
	let length = match (n_policy, m, c) {
		(Fixed(l), _, _) => Some(l),
		(Available(q), Some(l), _) => Some(((l as f64) * q) as usize),
		(WrapContent(_min, _max), _, _) => {
			let same_axis = (n_container, p_container).same_axis_or_both_none();
			let (m, c) = match same_axis {
				true  => (m, c),
				false => (None, None),
			};
			let children = node.children();
			let lengthy_children = children.iter().filter_map(|child| {
				let mut child = lock(child)?;
				compute_node(child.deref_mut(), node, m, c, &mut children_cursor)
			});
			match same_axis {
				false => lengthy_children.max(),
				true => Some({
					let mut sum = 0;
					let mut count: usize = 0;
					for len in lengthy_children {
						count += 1;
						sum += len;
					}
					let gaps = match (count.checked_sub(1), n_container) {
						(Some(l), Some((_, gap))) => l * gap,
						_ => 0,
					};
					sum + gaps
				}),
			}
		},
		(Chunks(r), _, Some(l)) if !(n_container, p_container).same_axis_or_both_none() => {
			// n_container could be None and it would create
			// an empty Chunks container... not an issue?
			let (_, gap) = n_container?;
			let (m, c) = (Some(l), Some(r));
			let children = node.children();
			let lengthy_children = children.iter().filter_map(|child| {
				let mut child = lock(child)?;
				compute_node(child.deref_mut(), node, m, c, &mut children_cursor)
			});
			let mut chunks = 1;
			let mut chunk_length = 0;
			for child_length in lengthy_children {
				let new_chunk_length = chunk_length + child_length + gap;
				if new_chunk_length > l {
					chunks += 1;
					chunk_length = child_length;
				} else {
					chunk_length = new_chunk_length;
				}
			}
			Some(chunks * r)
		},
		(AspectRatio(r), _, Some(l)) => {
			let result = match p_container?.0 {
				Horizontal => (l as f64) * r,
				Vertical => (l as f64) / r,
			};
			match result.is_finite() && result >= 0.0 {
				true => Some(result as usize),
				false => Some(0),
			}
		},
		_ => None,
	};

	let size = match (length, c, p_container) {
		(Some(l), Some(c), Some((Horizontal, _))) => Some((l, c)),
		(Some(l), Some(c), Some((Vertical,   _))) => Some((c, l)),
		_ => None,
	};

	if let Some((w, h)) = size {
		// size is Some -> length & p_container are Some.
		let l = length.unwrap();
		let (p_axis, _) = p_container.unwrap();
		if let Chunks(r) = p.policy() {
			let (p_position, p_size) = p.get_spot();
			let (pos, len, dstm, dstc) = match p_axis {
				Horizontal => (p_position.x, p_size.w, &mut cursor.x, &mut cursor.y),
				Vertical   => (p_position.y, p_size.h, &mut cursor.y, &mut cursor.x),
			};
			if *dstm + (l as isize) > (pos + (len as isize)) {
				*dstc += r as isize;
				*dstm = pos;
			}
		}
		node.set_spot((*cursor, Size::new(w, h)));
		cursor.add_to_axis(p_axis, l as isize);
	}

	length
}
