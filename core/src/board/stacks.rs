//! Branch relationships among the bounded set of loaded PRs. No ancestry
//! guesses, labels or additional requests; native stack metadata wins.

use std::collections::HashMap;

use super::{BoardRow, BranchLayer, StackInfo};

pub(super) fn infer_branch_stacks(rows: &mut [BoardRow]) {
    // Load more can supply a missing parent, or replace a retargeted PR.
    // Rebuild inferred metadata from facts, never from the previous tree.
    for row in rows.iter_mut() {
        if row.stack.as_ref().is_some_and(|s| s.branch.is_some()) {
            row.stack = None;
        }
    }
    let mut heads = HashMap::new();
    for (i, row) in rows.iter().enumerate() {
        let (Some(repo), Some(head)) = (&row.head_repo, &row.head_ref_name) else {
            continue;
        };
        // A fork's same-named branch is not a branch in the base repository.
        if !repo.eq_ignore_ascii_case(&row.repo) || head.is_empty() {
            continue;
        }
        heads
            .entry((repo.to_ascii_lowercase(), head.as_str()))
            .and_modify(|parent| *parent = None) // ambiguous: do not choose a PR
            .or_insert(Some(i));
    }
    let parents: Vec<Option<usize>> = rows
        .iter()
        .map(|row| {
            if row.stack.is_some() {
                return None;
            }
            let base = row.base_ref_name.as_deref()?;
            let parent = heads
                .get(&(row.repo.to_ascii_lowercase(), base))
                .copied()
                .flatten()?;
            rows[parent].stack.is_none().then_some(parent)
        })
        .collect();
    let mut children = vec![Vec::new(); rows.len()];
    for (i, parent) in parents.iter().enumerate() {
        if let Some(parent) = parent {
            children[*parent].push(i);
        }
    }
    for siblings in &mut children {
        siblings.sort_by_key(|&i| rows[i].number);
    }
    // Every node has at most one parent. Walking only roots excludes cycles
    // and their descendants, without recursion or an unbounded visited cache.
    for root in 0..rows.len() {
        if parents[root].is_some() || children[root].is_empty() {
            continue;
        }
        let mut pending = vec![(root, 0)];
        let mut members = Vec::new();
        while let Some((i, depth)) = pending.pop() {
            members.push((i, depth));
            pending.extend(children[i].iter().rev().map(|&child| (child, depth + 1)));
        }
        let size = members.len() as u64;
        let number = rows[root].number;
        for (position, (i, depth)) in members.into_iter().enumerate() {
            rows[i].stack = Some(StackInfo {
                number,
                size,
                base_ref_name: rows[i].base_ref_name.clone().unwrap_or_default(),
                position: Some(position as u64 + 1),
                branch: Some(BranchLayer {
                    parent: parents[i].map(|p| rows[p].number),
                    depth,
                }),
            });
        }
    }
}
