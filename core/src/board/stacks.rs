//! Branch relationships among the bounded set of loaded PRs. No ancestry
//! guesses, labels or additional requests; native stack metadata wins.

use std::collections::HashMap;

use super::{BoardRow, BranchLayer, StackInfo};

pub(super) fn infer_branch_stacks(rows: &mut [BoardRow]) {
    // Load more can supply a missing parent, or replace a retargeted PR.
    // Rebuild inferred metadata from facts, never from the previous tree.
    for row in rows.iter_mut() {
        row.base_pr = None;
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
    let mut parents: Vec<Option<usize>> = rows
        .iter()
        .map(|row| {
            let base = row.base_ref_name.as_deref()?;
            heads
                .get(&(row.repo.to_ascii_lowercase(), base))
                .copied()
                .flatten()
        })
        .collect();
    let mut children = vec![Vec::new(); rows.len()];
    for (i, parent) in parents.iter().enumerate() {
        if let Some(parent) = parent {
            children[*parent].push(i);
        }
    }
    // Dependencies and groups are different facts. A plain PR can target a
    // native-stack layer without becoming a made-up layer in that stack.
    // Root walks omit cycles and their descendants, as grouping does below.
    for root in 0..rows.len() {
        if parents[root].is_some() {
            continue;
        }
        let mut pending = vec![root];
        while let Some(i) = pending.pop() {
            rows[i].base_pr = parents[i].map(|parent| rows[parent].number);
            pending.extend(children[i].iter().copied());
        }
    }
    // Group only the non-native segments; native metadata always wins.
    for (i, parent) in parents.iter_mut().enumerate() {
        if rows[i].stack.is_some()
            || parent.is_some_and(|p| rows[p].stack.is_some() || rows[i].base_pr.is_none())
        {
            // A native boundary must not turn part of a rejected cycle into
            // a new inferred tree, including descendants of that cycle.
            *parent = None;
        }
    }
    for siblings in &mut children {
        siblings.clear();
    }
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
