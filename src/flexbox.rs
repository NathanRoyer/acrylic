use crate::tree::Tree;
use crate::tree::NodeKey;
use crate::tree::Axis::*;
use crate::tree::LengthPolicy::*;
use crate::Point;
use crate::Size;

use crate::tree::SameAxisContainerOrNone;

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
pub fn compute_tree(t: &mut Tree, root: NodeKey) {
	let (orig_pt, n_size) = t.get_node_spot(root).expect("flexbox error: root node has no spot!");
	let (axis, gap) = t.get_node_container(root).expect("flexbox error: root node is not a container!");
	let mut pt = orig_pt;
	let (m, c) = match axis {
		Horizontal => (n_size.w, n_size.h),
		Vertical   => (n_size.h, n_size.w),
	};
	let mut occupied = 0;
	for i in t.children(root) {
		if let Some(l) = compute_nodes(t, i, root, None, Some(c), &mut pt) {
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
	for i in t.children(root) {
		compute_nodes(t, i, root, Some(available), Some(c), &mut pt);
		pt.add_to_axis(axis, gap as isize);
	}
}

fn compute_nodes(t: &mut Tree, i: NodeKey, p: NodeKey, m: Option<usize>, c: Option<usize>, cursor: &mut Point) -> Option<usize> {
	let original_cursor = *cursor;
	let length = compute_node(t, i, p, m, c, cursor);
	if let Some((axis, gap)) = t.get_node_container(i) {
		let (p_axis, _) = t.get_node_container(p).unwrap();
		let backup = *cursor;
		*cursor = original_cursor;
		let n_policy = t.get_node_policy(i);
		let same_axis = axis == p_axis;
		let (m, c) = match n_policy {
			Some(Chunks(r)) => match same_axis {
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
		for j in t.children(i) {
			if let Some(l) = compute_nodes(t, j, i, None, c, cursor) {
				occupied += l;
			}
			occupied += gap;
			cursor.add_to_axis(axis, gap as isize);
		}
		if occupied != 0 {
			occupied -= gap;
		}
		if let Some(total) = m {
			if let Some(available) = total.checked_sub(occupied) {
				*cursor = original_cursor;
				for j in t.children(i) {
					compute_nodes(t, j, i, Some(available), c, cursor);
					cursor.add_to_axis(axis, gap as isize);
				}
			}
		}
		*cursor = backup;
	}
	length
}

fn compute_node(t: &mut Tree, i: NodeKey, p: NodeKey, m: Option<usize>, c: Option<usize>, cursor: &mut Point) -> Option<usize> {
	let mut children_cursor = *cursor;
	let n_policy = t.get_node_policy(i)?;
	let n_container  = t.get_node_container(i);
	let p_container = t.get_node_container(p);
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
			let children = t.children(i);
			let lengthy_children = children.iter().filter_map(|j| {
				compute_node(t, *j, i, m, c, &mut children_cursor)
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
		(Chunks(r), _, Some(l)) if (n_container, p_container).same_axis_or_both_none() => {
			// n_container could be None and it would create
			// an empty Chunks container... not an issue?
			let (m, c) = (Some(l), Some(r));
			let children = t.children(i);
			let lengthy_children = children.iter().filter_map(|j| {
				compute_node(t, *j, i, m, c, &mut children_cursor)
			});
			let mut chunks = 1;
			let mut chunk_length = 0;
			for child_length in lengthy_children {
				let new_chunk_length = chunk_length + child_length;
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
		let (p_container, _) = p_container.unwrap();
		// pay attention to this line:
		let (p_position, p_size) = t.get_node_spot(p)?;
		let p_policy = t.get_node_policy(p);
		{
			let (pos, max, dstm, dstc) = match p_container {
				Horizontal => (p_position.x, p_size.w, &mut cursor.x, &mut cursor.y),
				Vertical   => (p_position.y, p_size.h, &mut cursor.y, &mut cursor.x),
			};
			if let Some(Chunks(r)) = p_policy {
				if *dstm + (l as isize) > (pos + (max as isize)) {
					*dstc += r as isize;
					*dstm = pos;
				}
			}
		}
		// this looks unsafe but we're sure `i` wont change:
		// if node had no spot, previous commented line would
		// have made us return
		let mut i = i;
		t.set_node_spot(&mut i, Some((*cursor, Size::new(w, h))));

		cursor.add_to_axis(p_container, l as isize);
	}

	length
}
