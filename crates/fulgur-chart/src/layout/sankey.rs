//! sankey レイアウト。chartjs-chart-sankey `lib/{core,layout}.ts` の忠実移植。
//!
//! 決定性の不変条件(設計の要点):
//! - ノードは挿入順(初出データ順)。`Vec<Node>` + `HashMap<String, usize>`(キー→index)で表す。
//!   出力順に影響する走査はすべて `Vec` を index 順で辿る(`HashMap` は参照専用)。
//! - 全ソートは安定(`slice::sort_by`)。tie-break を chartjs から忠実移植する。
//! - グローバル可変カウンタ(`getCountId`/`_visited`)は移植せず、走査ごとの `HashSet<usize>` を使う。
//! - `Edge.node` は相手ノードの **index**(chartjs のポインタの代替)。

use super::common::{OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT};
use crate::ir::{
    ChartKind, ChartSpec, Color, SankeyColorMode, SankeyLink, SankeyModeX, SankeySize,
};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::collections::{HashMap, HashSet};

/// chartjs `SMALL_VALUE`。同 y への積み重なりを避ける微小オフセット。
const SMALL_VALUE: f64 = 1e-6;
/// ラベルとノードの間の固定ギャップ(chartjs `_drawLabels` の `+ 4`)。
const LABEL_GAP: f64 = 4.0;

/// 内部ノード。chartjs の Node オブジェクトに対応する。
#[derive(Clone, Debug)]
struct Node {
    key: String,
    /// chartjs `in`(入力フロー総和)。
    in_flow: f64,
    /// chartjs `out`(出力フロー総和)。
    out_flow: f64,
    size: f64,
    /// 列。未割当=None(chartjs の `defined(node.x)`)。
    x: Option<usize>,
    /// 縦位置。未割当=None。
    y: Option<f64>,
    priority: Option<f64>,
    /// chartjs `node.column`(手動 x 指定フラグ)。
    has_manual_column: bool,
    /// 入力リンク。
    from: Vec<Edge>,
    /// 出力リンク。
    to: Vec<Edge>,
}

/// 内部エッジ。chartjs の flow オブジェクトに対応する。`node` は相手ノードの index。
#[derive(Clone, Debug)]
struct Edge {
    flow: f64,
    index: usize,
    key: String,
    /// 相手ノードの index(`from` なら入力元、`to` なら出力先)。
    node: usize,
    add_y: f64,
}

/// `node.from` / `node.to` のどちら側を指すか。`nodeCount` の prop も兼ねる。
#[derive(Clone, Copy, PartialEq)]
enum Side {
    From,
    To,
}

// ────────────────────────────────────────────────
// 汎用ユーティリティ
// ────────────────────────────────────────────────

/// 安定ソート由来の置換 `order`(新→旧 index)を `v` に適用する。
/// 各 index はちょうど一度だけ使われる(置換)。
fn apply_perm<T>(v: &mut Vec<T>, order: &[usize]) {
    let mut items: Vec<Option<T>> = std::mem::take(v).into_iter().map(Some).collect();
    let mut out = Vec::with_capacity(order.len());
    for &idx in order {
        out.push(
            items[idx]
                .take()
                .expect("permutation index used exactly once"),
        );
    }
    *v = out;
}

/// chartjs の `(node.y + delta) || 0`(0/NaN は 0 に潰す)を移植する。
/// `y` が未定義(None)なら JS の `undefined + n = NaN` → `NaN || 0` = 0。
fn truthy_add(y: Option<f64>, delta: f64) -> f64 {
    match y {
        Some(yy) => {
            let v = yy + delta;
            if v != 0.0 && !v.is_nan() { v } else { 0.0 }
        }
        None => 0.0,
    }
}

// ────────────────────────────────────────────────
// ノード構築(core.ts: buildNodesFromData / setSizes / setPriorities / setColumns)
// ────────────────────────────────────────────────

/// `flowSort`: flow 降順、同値は index 昇順(安定)。
fn flow_sort(a: &Edge, b: &Edge) -> std::cmp::Ordering {
    if a.flow == b.flow {
        a.index.cmp(&b.index)
    } else {
        // chartjs: `b.flow - a.flow`(降順)。flow に NaN は無い(入力検証で排除済み)。
        // 万一 NaN が漏れても順序を壊さないよう Equal にフォールバック(plan 不変条件 #4)。
        b.flow
            .partial_cmp(&a.flow)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// 初出データ順でノードを構築し、size / priority / column を設定する。
/// 戻り値: 挿入順の `Vec<Node>` と key→index マップ。
fn build_nodes(
    data: &[SankeyLink],
    size_method: SankeySize,
    priority: &HashMap<String, f64>,
    columns: &HashMap<String, usize>,
) -> (Vec<Node>, HashMap<String, usize>) {
    let mut nodes: Vec<Node> = Vec::new();
    let mut idx: HashMap<String, usize> = HashMap::new();

    // key を初出時に Vec へ登録(from→to の順で走査するため chartjs の挿入順と一致)。
    fn get_or_create(nodes: &mut Vec<Node>, idx: &mut HashMap<String, usize>, key: &str) -> usize {
        if let Some(&i) = idx.get(key) {
            return i;
        }
        let i = nodes.len();
        nodes.push(Node {
            key: key.to_string(),
            in_flow: 0.0,
            out_flow: 0.0,
            size: 0.0,
            x: None,
            y: None,
            priority: None,
            has_manual_column: false,
            from: Vec::new(),
            to: Vec::new(),
        });
        idx.insert(key.to_string(), i);
        i
    }

    for (i, link) in data.iter().enumerate() {
        let from_idx = get_or_create(&mut nodes, &mut idx, &link.from);
        let to_idx = get_or_create(&mut nodes, &mut idx, &link.to);

        nodes[from_idx].out_flow += link.flow;
        nodes[from_idx].to.push(Edge {
            flow: link.flow,
            index: i,
            key: link.to.clone(),
            node: to_idx,
            add_y: 0.0,
        });
        nodes[to_idx].in_flow += link.flow;
        nodes[to_idx].from.push(Edge {
            flow: link.flow,
            index: i,
            key: link.from.clone(),
            node: from_idx,
            add_y: 0.0,
        });
    }

    // setSizes: from/to を flowSort で安定ソートし、size を算出する。
    for nd in nodes.iter_mut() {
        nd.from.sort_by(flow_sort);
        nd.to.sort_by(flow_sort);
        // `Math[size](node.in || node.out, node.out || node.in)`。
        let a = if nd.in_flow != 0.0 {
            nd.in_flow
        } else {
            nd.out_flow
        };
        let b = if nd.out_flow != 0.0 {
            nd.out_flow
        } else {
            nd.in_flow
        };
        nd.size = match size_method {
            SankeySize::Max => a.max(b),
            SankeySize::Min => a.min(b),
        };
    }

    // setPriorities / setColumns(参照専用 HashMap のルックアップ)。
    for nd in nodes.iter_mut() {
        if let Some(&p) = priority.get(&nd.key) {
            nd.priority = Some(p);
        }
        if let Some(&c) = columns.get(&nd.key) {
            nd.has_manual_column = true;
            nd.x = Some(c);
        }
    }

    (nodes, idx)
}

// ────────────────────────────────────────────────
// 列(x)割り当て(layout.ts: getAllKeysForward / startColumn / nextColumn / calculateX)
// ────────────────────────────────────────────────

/// `start` から `to` を前方に辿って到達可能なノード index を返す。
/// visited は走査ごとに新規の `HashSet<usize>`(グローバルカウンタは使わない)。
fn get_all_keys_forward(
    nodes: &[Node],
    start: &[usize],
    visited: &mut HashSet<usize>,
) -> Vec<usize> {
    let mut keys = Vec::new();
    for &node_idx in start {
        if !visited.insert(node_idx) {
            continue;
        }
        keys.push(node_idx);
        let next: Vec<usize> = nodes[node_idx].to.iter().map(|e| e.node).collect();
        keys.extend(get_all_keys_forward(nodes, &next, visited));
    }
    keys
}

/// x=0 の列。入力の無いノード + 循環打破のためデータ順に補う起点。
fn start_column(
    nodes: &[Node],
    data: &[SankeyLink],
    key_to_idx: &HashMap<String, usize>,
) -> Vec<usize> {
    let start_nodes: Vec<usize> = (0..nodes.len())
        .filter(|&i| nodes[i].from.is_empty())
        .collect();
    let mut column = start_nodes.clone();
    // `referencedNodes = new Set(getAllKeysForward(startNodes))`。
    let mut referenced: HashSet<usize> = HashSet::new();
    let _ = get_all_keys_forward(nodes, &start_nodes, &mut referenced);
    for point in data {
        let from_idx = key_to_idx[&point.from];
        let to_idx = key_to_idx[&point.to];
        if !referenced.contains(&from_idx) && !referenced.contains(&to_idx) {
            column.push(from_idx);
            referenced.insert(from_idx);
        }
        referenced.insert(to_idx);
    }
    column
}

/// 次列。残キーのうち「残キーから to されていない」ものを選ぶ。無ければ残キー先頭 1 つ(循環打破)。
fn next_column(n: usize, data_no_loops: &[(usize, usize)], placed: &[bool]) -> Vec<usize> {
    let mut remaining_to: HashSet<usize> = HashSet::new();
    for &(f, t) in data_no_loops {
        if !placed[f] {
            remaining_to.insert(t);
        }
    }
    let remaining: Vec<usize> = (0..n).filter(|&i| !placed[i]).collect();
    let cols_not_in_to: Vec<usize> = remaining
        .iter()
        .copied()
        .filter(|i| !remaining_to.contains(i))
        .collect();
    if !cols_not_in_to.is_empty() {
        cols_not_in_to
    } else {
        remaining.into_iter().take(1).collect()
    }
}

/// 各ノードに列(x)を割り当て、maxX を返す。
fn calculate_x(
    nodes: &mut [Node],
    data: &[SankeyLink],
    key_to_idx: &HashMap<String, usize>,
    mode: SankeyModeX,
) -> usize {
    let n = nodes.len();
    if n == 0 {
        return 0;
    }
    let data_no_loops: Vec<(usize, usize)> = data
        .iter()
        .filter(|d| d.from != d.to)
        .map(|d| (key_to_idx[&d.from], key_to_idx[&d.to]))
        .collect();

    let mut placed = vec![false; n];
    let mut remaining = n;
    let mut x = 0usize;
    // 各反復は必ず 1 ノード以上を配置するため、有効グラフでは最大 n 反復で完了する。
    // cap は upstream の "Fatal error: unable to place nodes" 相当の保険であり、
    // 有効グラフでは到達しない(無限ループに対する唯一のガード)。
    let cap = n + 1;
    let mut iterations = 0usize;

    while remaining > 0 {
        iterations += 1;
        if iterations > cap {
            debug_assert!(
                false,
                "sankey calculate_x exceeded iteration cap (unreachable for valid graphs)"
            );
            break;
        }
        let column = if x == 0 {
            start_column(nodes, data, key_to_idx)
        } else {
            next_column(n, &data_no_loops, &placed)
        };
        // 有効グラフでは column は常に非空(start_column は n>=1 で非空、next_column は
        // 残キーがある限り slice(0,1) フォールバックで非空)。
        debug_assert!(
            !column.is_empty(),
            "sankey: unable to place nodes to columns"
        );
        for key in column {
            if !placed[key] {
                if nodes[key].x.is_none() {
                    nodes[key].x = Some(x);
                }
                placed[key] = true;
                remaining -= 1;
            }
        }
        if remaining > 0 {
            x += 1;
        }
    }

    let max_x = nodes.iter().map(|nd| nd.x.unwrap_or(0)).max().unwrap_or(0);

    if mode == SankeyModeX::Edge {
        // 出力(from)を持たないノードを右端へ寄せる(手動列は除く)。
        let from_keys: HashSet<usize> = data.iter().map(|d| key_to_idx[&d.from]).collect();
        for (i, nd) in nodes.iter_mut().enumerate() {
            if !from_keys.contains(&i) && !nd.has_manual_column {
                nd.x = Some(max_x);
            }
        }
    }

    max_x
}

// ────────────────────────────────────────────────
// y 割り当て(layout.ts: nodeCount / processFrom / processTo / processRest / fixTop / findStartNode / calculateY)
// ────────────────────────────────────────────────

/// `side` 方向に到達可能なノード数を数える(visited は走査ごとに新規)。
fn node_count(nodes: &[Node], list: &[Edge], side: Side, visited: &mut HashSet<usize>) -> usize {
    let mut count = 0;
    for e in list {
        if !visited.insert(e.node) {
            continue;
        }
        let next: &[Edge] = match side {
            Side::From => &nodes[e.node].from,
            Side::To => &nodes[e.node].to,
        };
        count += next.len() + node_count(nodes, next, side, visited);
    }
    count
}

/// `flowByNodeCount(side)`: nodeCount 昇順、同値は対象ノードの `[side]` 長さ昇順(安定)。
/// nodeCount は順序非依存なので、ソート前にキーを先取りしてから置換を適用する
/// (`nodes` を可変借用したまま他ノードを読む借用衝突を回避する。観測挙動は不変)。
fn sort_by_node_count(nodes: &mut [Node], i: usize, side: Side) {
    let len = match side {
        Side::From => nodes[i].from.len(),
        Side::To => nodes[i].to.len(),
    };
    if len <= 1 {
        return;
    }
    let targets: Vec<usize> = match side {
        Side::From => nodes[i].from.iter().map(|e| e.node).collect(),
        Side::To => nodes[i].to.iter().map(|e| e.node).collect(),
    };
    let keys: Vec<(usize, usize)> = targets
        .iter()
        .map(|&t| {
            let mut visited = HashSet::new();
            let (list, list_len): (&[Edge], usize) = match side {
                Side::From => (&nodes[t].from, nodes[t].from.len()),
                Side::To => (&nodes[t].to, nodes[t].to.len()),
            };
            let nc = node_count(nodes, list, side, &mut visited);
            (nc, list_len)
        })
        .collect();
    let mut order: Vec<usize> = (0..len).collect();
    order.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
    match side {
        Side::From => apply_perm(&mut nodes[i].from, &order),
        Side::To => apply_perm(&mut nodes[i].to, &order),
    }
}

/// 入力側を辿って y を割り当てる(再帰)。y 未設定ガードで循環を自然終了する。
fn process_from(nodes: &mut [Node], node_idx: usize, mut y: f64) -> f64 {
    if nodes[node_idx].from.is_empty() {
        return y;
    }
    sort_by_node_count(nodes, node_idx, Side::From);
    let targets: Vec<usize> = nodes[node_idx].from.iter().map(|e| e.node).collect();
    for nidx in targets {
        if nodes[nidx].y.is_none() {
            nodes[nidx].y = Some(y);
            let next = if y != 0.0 { y + SMALL_VALUE } else { 0.0 };
            process_from(nodes, nidx, next);
        }
        y = (nodes[nidx].y.unwrap() + nodes[nidx].out_flow).max(y);
    }
    nodes[node_idx].y.unwrap_or(0.0) + nodes[node_idx].size
}

/// 出力側を辿って y を割り当てる(再帰)。
fn process_to(nodes: &mut [Node], node_idx: usize, mut y: f64) -> f64 {
    if nodes[node_idx].to.is_empty() {
        return y;
    }
    sort_by_node_count(nodes, node_idx, Side::To);
    let targets: Vec<usize> = nodes[node_idx].to.iter().map(|e| e.node).collect();
    for nidx in targets {
        if nodes[nidx].y.is_none() {
            nodes[nidx].y = Some(y);
            let next = if y != 0.0 { y + SMALL_VALUE } else { 0.0 };
            process_to(nodes, nidx, next);
        }
        y = (nodes[nidx].y.unwrap() + nodes[nidx].in_flow.max(nodes[nidx].out_flow)).max(y);
    }
    nodes[node_idx].y.unwrap_or(0.0) + nodes[node_idx].size
}

/// `setOrGetY`: y 未設定なら value を設定して返す。設定済みなら既存値を返す。
fn set_or_get_y(nodes: &mut [Node], i: usize, value: f64) -> f64 {
    if let Some(y) = nodes[i].y {
        y
    } else {
        nodes[i].y = Some(value);
        value
    }
}

/// start から到達できなかった残りノードの y を埋める。
fn process_rest(nodes: &mut [Node], max_x: usize) -> f64 {
    let n = nodes.len();
    let left_nodes: Vec<usize> = (0..n).filter(|&i| nodes[i].x == Some(0)).collect();
    let right_nodes: Vec<usize> = (0..n).filter(|&i| nodes[i].x == Some(max_x)).collect();
    // 分岐前にスナップショット(以後の処理で y が埋まる前の「未処理」集合)。
    let left_to_do: Vec<usize> = left_nodes
        .iter()
        .copied()
        .filter(|&i| nodes[i].y.is_none())
        .collect();
    let right_to_do: Vec<usize> = right_nodes
        .iter()
        .copied()
        .filter(|&i| nodes[i].y.is_none())
        .collect();
    let center_to_do: Vec<usize> = (0..n)
        .filter(|&i| nodes[i].x.is_some_and(|x| x > 0 && x < max_x) && nodes[i].y.is_none())
        .collect();

    let mut left_y = left_nodes.iter().fold(0.0_f64, |acc, &i| {
        acc.max(truthy_add(nodes[i].y, nodes[i].out_flow))
    }) + SMALL_VALUE;
    let mut right_y = right_nodes.iter().fold(0.0_f64, |acc, &i| {
        acc.max(truthy_add(nodes[i].y, nodes[i].in_flow))
    }) + SMALL_VALUE;
    let mut center_y = 0.0_f64;

    if left_y >= right_y {
        for &i in &left_to_do {
            left_y = set_or_get_y(nodes, i, left_y);
            let with_out = left_y + nodes[i].out_flow;
            let pt = process_to(nodes, i, left_y);
            left_y = with_out.max(pt);
        }
        for &i in &right_to_do {
            right_y = set_or_get_y(nodes, i, right_y);
            let with_in = right_y + nodes[i].in_flow;
            let pf = process_from(nodes, i, right_y);
            right_y = with_in.max(pf);
        }
    } else {
        for &i in &left_to_do {
            left_y = set_or_get_y(nodes, i, left_y);
        }
        for &i in &right_to_do {
            right_y = set_or_get_y(nodes, i, right_y);
            let with_in = right_y + nodes[i].in_flow;
            let pf = process_from(nodes, i, right_y);
            right_y = with_in.max(pf);
        }
    }

    for &i in &center_to_do {
        let nx = nodes[i].x;
        let mut y = (0..n)
            .filter(|&j| nodes[j].x == nx && nodes[j].y.is_some())
            .fold(0.0_f64, |acc, j| {
                acc.max(nodes[j].y.unwrap() + nodes[j].in_flow.max(nodes[j].out_flow))
            });
        y = set_or_get_y(nodes, i, y);
        let with_in = y + nodes[i].in_flow;
        let pf = process_from(nodes, i, y);
        y = with_in.max(pf);
        let with_out = y + nodes[i].out_flow;
        let pt = process_to(nodes, i, y);
        y = with_out.max(pt);
        center_y = center_y.max(y);
    }

    left_y.max(right_y).max(center_y)
}

/// 列ごとに上詰めして重なりを解消し、maxY を返す。
fn fix_top(nodes: &mut [Node], max_x: usize) -> f64 {
    let mut max_y = 0.0_f64;
    for x in 0..=max_x {
        let mut col: Vec<usize> = (0..nodes.len())
            .filter(|&i| nodes[i].x == Some(x))
            .collect();
        col.sort_by(|&a, &b| {
            nodes[a]
                .y
                .unwrap_or(0.0)
                .partial_cmp(&nodes[b].y.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut min_y = 0.0_f64;
        for &i in &col {
            let y = nodes[i].y.unwrap_or(0.0);
            let ny = if y < min_y {
                nodes[i].y = Some(min_y);
                min_y
            } else {
                y
            };
            min_y = ny + nodes[i].size;
        }
        max_y = max_y.max(min_y);
    }
    max_y
}

/// 起点ノードを選ぶ: 最大 size。複数なら x 昇順で左端優先、無ければ右端、無ければ中央。
fn find_start_node(nodes: &[Node], max_x: usize) -> usize {
    let max_size = nodes
        .iter()
        .map(|n| n.size)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut biggest: Vec<usize> = (0..nodes.len())
        .filter(|&i| nodes[i].size == max_size)
        .collect();
    if biggest.len() == 1 {
        return biggest[0];
    }
    biggest.sort_by(|&a, &b| nodes[a].x.unwrap_or(0).cmp(&nodes[b].x.unwrap_or(0)));
    if nodes[biggest[0]].x.unwrap_or(0) == 0 {
        return biggest[0];
    }
    let last = *biggest.last().unwrap();
    if nodes[last].x.unwrap_or(0) == max_x {
        return last;
    }
    let mid = biggest.len() / 2;
    biggest[mid]
}

/// デフォルトの y 割り当て。maxY を返す。
fn calculate_y(nodes: &mut [Node], max_x: usize) -> f64 {
    if nodes.is_empty() {
        return 0.0;
    }
    let start = find_start_node(nodes, max_x);
    nodes[start].y = Some(0.0);
    process_from(nodes, start, 0.0);
    process_to(nodes, start, 0.0);
    process_rest(nodes, max_x);
    fix_top(nodes, max_x)
}

/// priority 指定時の y 割り当て。列ごとに priority 昇順で縦積み。
fn calculate_y_using_priority(nodes: &mut [Node], max_x: usize) -> f64 {
    let mut max_y = 0.0_f64;
    let mut next_y_start = 0.0_f64;
    for x in 0..=max_x {
        let mut y = next_y_start;
        let mut col: Vec<usize> = (0..nodes.len())
            .filter(|&i| nodes[i].x == Some(x))
            .collect();
        col.sort_by(|&a, &b| {
            let pa = nodes[a].priority.unwrap_or(0.0);
            let pb = nodes[b].priority.unwrap_or(0.0);
            pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
        });
        // 次列の起点: 列先頭ノードが x+1 を飛び越す to のフロー総和。
        next_y_start = if let Some(&first) = col.first() {
            nodes[first]
                .to
                .iter()
                .filter(|e| nodes[e.node].x.is_some_and(|xx| xx > x + 1))
                .map(|e| e.flow)
                .sum()
        } else {
            0.0
        };
        for &i in &col {
            nodes[i].y = Some(y);
            y += nodes[i].out_flow.max(nodes[i].in_flow);
        }
        max_y = y.max(max_y);
    }
    max_y
}

// ────────────────────────────────────────────────
// padding と flow オフセット(layout.ts: nodeByXYSize / addPadding / sortFlows)
// ────────────────────────────────────────────────

/// ノード間に padding を挿入し、maxY を返す。
fn add_padding(nodes: &mut [Node], padding: f64) -> f64 {
    let mut max_y = 0.0_f64;
    // nodeByXYSize(x→y→size)で安定ソートした走査順。
    let mut order: Vec<usize> = (0..nodes.len()).collect();
    order.sort_by(|&a, &b| {
        let ax = nodes[a].x.unwrap_or(0);
        let bx = nodes[b].x.unwrap_or(0);
        if ax != bx {
            return ax.cmp(&bx);
        }
        let ay = nodes[a].y.unwrap_or(0.0);
        let by = nodes[b].y.unwrap_or(0.0);
        if ay == by {
            return nodes[a]
                .size
                .partial_cmp(&nodes[b].size)
                .unwrap_or(std::cmp::Ordering::Equal);
        }
        ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut column_xs: HashMap<usize, usize> = HashMap::new();
    let mut grid: Vec<Vec<f64>> = Vec::new();

    for &i in &order {
        let x = nodes[i].x.unwrap_or(0);
        let col_idx = *column_xs.entry(x).or_insert_with(|| {
            grid.push(Vec::new());
            grid.len() - 1
        });
        let node_y = nodes[i].y.unwrap_or(0.0);
        if node_y != 0.0 {
            grid[col_idx].push(node_y);
            let mut paddings = grid[col_idx].len();
            if nodes[i].in_flow != 0.0 {
                for other in grid.iter().take(col_idx) {
                    for (row, &val) in other.iter().enumerate() {
                        if val > node_y {
                            break;
                        }
                        paddings = paddings.max(row + 1);
                    }
                }
                while grid[col_idx].len() < paddings {
                    grid[col_idx].push(node_y);
                }
            }
            nodes[i].y = Some(node_y + paddings as f64 * padding);
        }
        let ny = nodes[i].y.unwrap_or(0.0);
        max_y = max_y.max(ny + nodes[i].in_flow.max(nodes[i].out_flow));
    }
    max_y
}

/// 各ノードの from/to を中心 y 昇順に並べ、各エッジの add_y(リボン端点オフセット)を計算する。
fn sort_flows(nodes: &mut [Node]) {
    for i in 0..nodes.len() {
        let node_size = nodes[i].size;
        let overlap_from = node_size < nodes[i].in_flow;
        let overlap_to = node_size < nodes[i].out_flow;

        // from: 入力元ノードの y + out/2 昇順。
        let m = nodes[i].from.len();
        if m > 0 {
            let keys: Vec<f64> = (0..m)
                .map(|k| {
                    let t = nodes[i].from[k].node;
                    nodes[t].y.unwrap_or(0.0) + nodes[t].out_flow / 2.0
                })
                .collect();
            let mut order: Vec<usize> = (0..m).collect();
            order.sort_by(|&a, &b| {
                keys[a]
                    .partial_cmp(&keys[b])
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            apply_perm(&mut nodes[i].from, &order);
            let mut add_y = 0.0_f64;
            let len = m as f64;
            for (idx, e) in nodes[i].from.iter_mut().enumerate() {
                if overlap_from {
                    // upstream `i * (node.size - flow) / (len - 1)` の忠実移植。
                    // size<in は size='min' でのみ起き、len==1 だと 0/0=NaN(upstream のバグ)。
                    // ここでガードを足すと挙動が分岐するため足さない。fmt_num が NaN→"0" に潰す。
                    e.add_y = (idx as f64 * (node_size - e.flow)) / (len - 1.0);
                } else {
                    e.add_y = add_y;
                    add_y += e.flow;
                }
            }
        }

        // to: 出力先ノードの y + in/2 昇順。
        let m = nodes[i].to.len();
        if m > 0 {
            let keys: Vec<f64> = (0..m)
                .map(|k| {
                    let t = nodes[i].to[k].node;
                    nodes[t].y.unwrap_or(0.0) + nodes[t].in_flow / 2.0
                })
                .collect();
            let mut order: Vec<usize> = (0..m).collect();
            order.sort_by(|&a, &b| {
                keys[a]
                    .partial_cmp(&keys[b])
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            apply_perm(&mut nodes[i].to, &order);
            let mut add_y = 0.0_f64;
            let len = m as f64;
            for (idx, e) in nodes[i].to.iter_mut().enumerate() {
                if overlap_to {
                    // upstream `i * (node.size - flow) / (len - 1)` の忠実移植。
                    // size<out は size='min' でのみ起き、len==1 だと 0/0=NaN(upstream のバグ)。
                    // ここでガードを足すと挙動が分岐するため足さない。fmt_num が NaN→"0" に潰す。
                    e.add_y = (idx as f64 * (node_size - e.flow)) / (len - 1.0);
                } else {
                    e.add_y = add_y;
                    add_y += e.flow;
                }
            }
        }
    }
}

/// chartjs `layout()` 相当。(maxX, maxY) を返し、ノードの x/y/add_y を確定させる。
fn layout(
    nodes: &mut [Node],
    data: &[SankeyLink],
    key_to_idx: &HashMap<String, usize>,
    use_priority: bool,
    height: f64,
    node_padding: f64,
    mode_x: SankeyModeX,
) -> (usize, f64) {
    let max_x = calculate_x(nodes, data, key_to_idx, mode_x);
    let max_y = if use_priority {
        calculate_y_using_priority(nodes, max_x)
    } else {
        calculate_y(nodes, max_x)
    };
    let padding = if height > 0.0 {
        (max_y / height) * node_padding
    } else {
        0.0
    };
    let max_y_padded = add_padding(nodes, padding);
    sort_flows(nodes);
    (max_x, max_y_padded)
}

// ────────────────────────────────────────────────
// シーン生成(controller.ts / flow.ts)
// ────────────────────────────────────────────────

/// `getAddY`: edges の中から key と index が一致する add_y を返す。無ければ 0。
fn get_add_y(edges: &[Edge], key: &str, index: usize) -> f64 {
    edges
        .iter()
        .find(|e| e.key == key && e.index == index)
        .map(|e| e.add_y)
        .unwrap_or(0.0)
}

/// 色に alpha を上書きする(RGB は保持)。
fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

/// 閉じたリボンのパス d を組む。全数値 `fmt_num`・空白区切り。
fn ribbon_path(x: f64, y: f64, x2: f64, y2: f64, height: f64) -> String {
    // flow.ts controlPoints。
    let (cp1x, cp1y, cp2x, cp2y) = if x < x2 {
        (x + (x2 - x) / 3.0 * 2.0, y, x + (x2 - x) / 3.0, y2)
    } else {
        (x - (x - x2) / 3.0, 0.0, x2 + (x - x2) / 3.0, 0.0)
    };
    format!(
        "M {} {} C {} {} {} {} {} {} L {} {} C {} {} {} {} {} {} Z",
        fmt_num(x),
        fmt_num(y),
        fmt_num(cp1x),
        fmt_num(cp1y),
        fmt_num(cp2x),
        fmt_num(cp2y),
        fmt_num(x2),
        fmt_num(y2),
        fmt_num(x2),
        fmt_num(y2 + height),
        fmt_num(cp2x),
        fmt_num(cp2y + height),
        fmt_num(cp1x),
        fmt_num(cp1y + height),
        fmt_num(x),
        fmt_num(y + height),
    )
}

/// 矩形の枠線パス d(空白区切り)。
fn rect_outline_path(x: f64, y: f64, w: f64, h: f64) -> String {
    format!(
        "M {} {} L {} {} L {} {} L {} {} Z",
        fmt_num(x),
        fmt_num(y),
        fmt_num(x + w),
        fmt_num(y),
        fmt_num(x + w),
        fmt_num(y + h),
        fmt_num(x),
        fmt_num(y + h),
    )
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    // sankey 設定値を取り出す。labels/priority/columns は参照のまま、他は Copy。
    let ChartKind::Sankey {
        color_from,
        color_to,
        color_mode,
        alpha,
        node_width,
        node_padding,
        mode_x,
        size: size_method,
        border,
        border_width,
        label_color,
        labels,
        priority,
        columns,
    } = &spec.kind
    else {
        unreachable!("sankey::build called with non-sankey kind");
    };
    let (color_from, color_to, color_mode, alpha) = (*color_from, *color_to, *color_mode, *alpha);
    let (node_width, node_padding, mode_x, size_method) =
        (*node_width, *node_padding, *mode_x, *size_method);
    let (border, border_width, label_color) = (*border, *border_width, *label_color);

    let data: &[SankeyLink] = spec
        .series
        .first()
        .map(|s| s.links.as_slice())
        .unwrap_or(&[]);

    let (mut nodes, key_to_idx) = build_nodes(data, size_method, priority, columns);
    let use_priority = !priority.is_empty();
    let (max_x, max_y) = layout(
        &mut nodes,
        data,
        &key_to_idx,
        use_priority,
        spec.height,
        node_padding,
        mode_x,
    );

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    // 表示ラベル文字列(labels マップで上書き)と最大幅。
    let display_label =
        |key: &str| -> String { labels.get(key).cloned().unwrap_or_else(|| key.to_string()) };
    let mut max_label_w = 0.0_f32;
    for nd in &nodes {
        let w = m.width(&display_label(&nd.key), label_font as f32);
        if w > max_label_w {
            max_label_w = w;
        }
    }
    let label_margin = max_label_w as f64;

    // プロット領域(タイトル帯を上に確保、左右にラベル用マージン)。
    let title_band = if spec.title.is_some() {
        TITLE_BAND
    } else {
        0.0
    };
    let plot_left = OUTER_PAD + label_margin;
    let plot_right = spec.width - OUTER_PAD - label_margin;
    let plot_top = OUTER_PAD + title_band;
    let plot_bottom = spec.height - OUTER_PAD;
    let plot_w = (plot_right - plot_left).max(0.0);
    let plot_h = (plot_bottom - plot_top).max(0.0);
    let plot_mid = (plot_left + plot_right) / 2.0;

    let max_x_f = max_x as f64;
    // x/y 写像(maxX==0 → plot_left、maxY==0 → plot_top)。y は 0 が上。
    let px = |xv: f64| -> f64 {
        if max_x == 0 {
            plot_left
        } else {
            plot_left + (xv / max_x_f) * plot_w
        }
    };
    let py = |yv: f64| -> f64 {
        if max_y <= 0.0 {
            plot_top
        } else {
            plot_top + (yv / max_y) * plot_h
        }
    };

    // ノード塗り色: 各リンクで from=colorFrom / to=colorTo を順に上書きし最後が残る(alpha 無し)。
    let mut node_color: Vec<Color> = vec![color_from; nodes.len()];
    for link in data {
        node_color[key_to_idx[&link.from]] = color_from;
        node_color[key_to_idx[&link.to]] = color_to;
    }

    let border_space = if border_width > 0.0 {
        border_width / 2.0 + 0.5
    } else {
        0.0
    };

    let mut items: Vec<Prim> = Vec::new();

    // タイトル。
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
            rotate_deg: None,
        });
    }

    // リボン(ノードより背面)。データ順。
    for (i, link) in data.iter().enumerate() {
        let from_idx = key_to_idx[&link.from];
        let to_idx = key_to_idx[&link.to];
        let from_x = nodes[from_idx].x.unwrap_or(0) as f64;
        let to_x = nodes[to_idx].x.unwrap_or(0) as f64;
        let from_y_val =
            nodes[from_idx].y.unwrap_or(0.0) + get_add_y(&nodes[from_idx].to, &link.to, i);
        let to_y_val =
            nodes[to_idx].y.unwrap_or(0.0) + get_add_y(&nodes[to_idx].from, &link.from, i);

        let x = px(from_x) + node_width + border_space;
        let x2 = px(to_x) - border_space;
        let y = py(from_y_val);
        let y2 = py(to_y_val);
        let height = (py(from_y_val + link.flow) - y).abs();

        let d = ribbon_path(x, y, x2, y2, height);
        match color_mode {
            SankeyColorMode::From => items.push(Prim::Path {
                d,
                fill: Some(with_alpha(color_from, alpha)),
                stroke: None,
                stroke_width: 0.0,
            }),
            SankeyColorMode::To => items.push(Prim::Path {
                d,
                fill: Some(with_alpha(color_to, alpha)),
                stroke: None,
                stroke_width: 0.0,
            }),
            SankeyColorMode::Gradient => items.push(Prim::GradientPath {
                d,
                x0: x,
                x1: x2,
                stop0: with_alpha(color_from, alpha),
                stop1: with_alpha(color_to, alpha),
            }),
        }
    }

    // ノード矩形 + 枠線。ノード(挿入)順。
    for (i, nd) in nodes.iter().enumerate() {
        let nx = nd.x.unwrap_or(0) as f64;
        let ny = nd.y.unwrap_or(0.0);
        let rx = px(nx);
        let ry = py(ny);
        let rh = py(ny + nd.size) - ry;
        items.push(Prim::Rect {
            x: rx,
            y: ry,
            w: node_width,
            h: rh,
            fill: node_color[i],
        });
        if border_width > 0.0 {
            items.push(Prim::Path {
                d: rect_outline_path(rx, ry, node_width, rh),
                fill: None,
                stroke: Some(border),
                stroke_width: border_width,
            });
        }
    }

    // ノードラベル。ノード(挿入)順。左半分なら右へ、右半分なら左へ。
    for nd in &nodes {
        let nx = nd.x.unwrap_or(0) as f64;
        let ny = nd.y.unwrap_or(0.0);
        let rx = px(nx);
        let ry = py(ny);
        let rh = py(ny + nd.size) - ry;
        let cy = ry + rh / 2.0 + label_font * TEXT_BASELINE_RATIO;
        let (anchor, tx) = if rx < plot_mid {
            (Anchor::Start, rx + node_width + border_width + LABEL_GAP)
        } else {
            (Anchor::End, rx - border_width - LABEL_GAP)
        };
        items.push(Prim::Text {
            x: tx,
            y: cy,
            size: label_font,
            anchor,
            fill: label_color,
            content: display_label(&nd.key),
            rotate_deg: None,
        });
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(spec: &[(&str, &str, f64)]) -> Vec<SankeyLink> {
        spec.iter()
            .map(|(f, t, fl)| SankeyLink {
                from: f.to_string(),
                to: t.to_string(),
                flow: *fl,
            })
            .collect()
    }

    fn hm_f() -> HashMap<String, f64> {
        HashMap::new()
    }
    fn hm_u() -> HashMap<String, usize> {
        HashMap::new()
    }

    fn keys_of(nodes: &[Node]) -> Vec<String> {
        nodes.iter().map(|n| n.key.clone()).collect()
    }

    // ── Task 3.1: ノード構築 ──

    #[test]
    fn builds_nodes_in_data_order_with_sizes() {
        let data = links(&[
            ("A", "B", 10.0),
            ("A", "C", 5.0),
            ("B", "C", 10.0),
            ("C", "D", 15.0),
        ]);
        let (nodes, idx) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        // 初出順 A,B,C,D。
        assert_eq!(keys_of(&nodes), ["A", "B", "C", "D"]);
        assert_eq!(idx["A"], 0);
        assert_eq!(idx["D"], 3);
        // in/out。
        assert_eq!(nodes[0].out_flow, 15.0); // A
        assert_eq!(nodes[0].in_flow, 0.0);
        assert_eq!(nodes[1].in_flow, 10.0); // B
        assert_eq!(nodes[1].out_flow, 10.0);
        assert_eq!(nodes[2].in_flow, 15.0); // C
        assert_eq!(nodes[2].out_flow, 15.0);
        assert_eq!(nodes[3].in_flow, 15.0); // D
        assert_eq!(nodes[3].out_flow, 0.0);
        // size = max(in||out, out||in)。
        assert_eq!(nodes[0].size, 15.0);
        assert_eq!(nodes[1].size, 10.0);
        assert_eq!(nodes[2].size, 15.0);
        assert_eq!(nodes[3].size, 15.0);
    }

    #[test]
    fn from_to_edges_sorted_by_flow_desc_then_index() {
        // C は B(10)/A(5) から入る → flow 降順で B が先。
        let data = links(&[("A", "C", 5.0), ("B", "C", 10.0)]);
        let (nodes, idx) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        let c = &nodes[idx["C"]];
        assert_eq!(c.from.len(), 2);
        assert_eq!(c.from[0].key, "B"); // flow 10 が先
        assert_eq!(c.from[1].key, "A"); // flow 5
    }

    #[test]
    fn min_size_method_uses_min() {
        // B: in=10, out=5 → min=5。
        let data = links(&[("A", "B", 10.0), ("B", "C", 5.0)]);
        let (nodes, idx) = build_nodes(&data, SankeySize::Min, &hm_f(), &hm_u());
        assert_eq!(nodes[idx["B"]].size, 5.0);
    }

    // ── Task 3.2: x 割り当て ──

    #[test]
    fn calculate_x_linear_chain() {
        let data = links(&[("A", "B", 1.0), ("B", "C", 1.0), ("C", "D", 1.0)]);
        let (mut nodes, idx) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        let max_x = calculate_x(&mut nodes, &data, &idx, SankeyModeX::Edge);
        assert_eq!(max_x, 3);
        assert_eq!(nodes[idx["A"]].x, Some(0));
        assert_eq!(nodes[idx["B"]].x, Some(1));
        assert_eq!(nodes[idx["C"]].x, Some(2));
        assert_eq!(nodes[idx["D"]].x, Some(3));
    }

    #[test]
    fn calculate_x_branch_same_column() {
        let data = links(&[("A", "B", 1.0), ("A", "C", 1.0)]);
        let (mut nodes, idx) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        let max_x = calculate_x(&mut nodes, &data, &idx, SankeyModeX::Edge);
        assert_eq!(max_x, 1);
        assert_eq!(nodes[idx["A"]].x, Some(0));
        assert_eq!(nodes[idx["B"]].x, nodes[idx["C"]].x);
        assert_eq!(nodes[idx["B"]].x, Some(1));
    }

    #[test]
    fn calculate_x_edge_mode_pushes_terminals_right() {
        // A→B, B→C, A→D。D は出力なしの末端。
        let data = links(&[("A", "B", 1.0), ("B", "C", 1.0), ("A", "D", 1.0)]);

        let (mut nodes_e, idx_e) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        let max_x_e = calculate_x(&mut nodes_e, &data, &idx_e, SankeyModeX::Edge);
        assert_eq!(max_x_e, 2);
        // edge: D は maxX(=2) へ。
        assert_eq!(nodes_e[idx_e["D"]].x, Some(2));
        assert_eq!(nodes_e[idx_e["C"]].x, Some(2));

        let (mut nodes_v, idx_v) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        let _ = calculate_x(&mut nodes_v, &data, &idx_v, SankeyModeX::Even);
        // even: D は自然な列(1)に残る。
        assert_eq!(nodes_v[idx_v["D"]].x, Some(1));
    }

    #[test]
    fn calculate_x_respects_manual_column() {
        let data = links(&[("A", "B", 1.0), ("B", "C", 1.0)]);
        let mut cols = HashMap::new();
        cols.insert("C".to_string(), 5usize);
        let (mut nodes, idx) = build_nodes(&data, SankeySize::Max, &hm_f(), &cols);
        let max_x = calculate_x(&mut nodes, &data, &idx, SankeyModeX::Edge);
        assert_eq!(nodes[idx["C"]].x, Some(5));
        assert_eq!(max_x, 5);
    }

    #[test]
    fn calculate_x_cycle_does_not_panic() {
        // 単純な循環 A→B→A。
        let data = links(&[("A", "B", 1.0), ("B", "A", 1.0)]);
        let (mut nodes, idx) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        let max_x = calculate_x(&mut nodes, &data, &idx, SankeyModeX::Edge);
        // パニックせず全ノードに x が付く。
        assert!(nodes[idx["A"]].x.is_some());
        assert!(nodes[idx["B"]].x.is_some());
        assert!(max_x <= 1);
    }

    // ── Task 3.3: y 割り当て(非重なり) ──

    #[test]
    fn calculate_y_no_overlap_within_column() {
        let data = links(&[
            ("Coal", "Electricity", 25.0),
            ("Gas", "Electricity", 15.0),
            ("Electricity", "Residential", 20.0),
            ("Electricity", "Industrial", 20.0),
        ]);
        let (mut nodes, _idx) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        let max_x = calculate_x(
            &mut nodes,
            &data,
            &SankeyKeyIdx::idx(&data),
            SankeyModeX::Edge,
        );
        let _max_y = calculate_y(&mut nodes, max_x);
        // 各列で y 昇順に並べたとき y_i + size_i <= y_{i+1}(重なりなし)。
        for x in 0..=max_x {
            let mut col: Vec<usize> = (0..nodes.len())
                .filter(|&i| nodes[i].x == Some(x))
                .collect();
            col.sort_by(|&a, &b| {
                nodes[a]
                    .y
                    .unwrap()
                    .partial_cmp(&nodes[b].y.unwrap())
                    .unwrap()
            });
            for w in col.windows(2) {
                let (a, b) = (w[0], w[1]);
                let bottom = nodes[a].y.unwrap() + nodes[a].size;
                assert!(
                    bottom <= nodes[b].y.unwrap() + 1e-6,
                    "overlap in column {x}: {bottom} > {}",
                    nodes[b].y.unwrap()
                );
            }
        }
    }

    #[test]
    fn calculate_y_disconnected_components_no_overlap() {
        // 2 つの独立フロー A→B と C→D(両者を繋ぐリンクなし)。
        // start ノードから到達できない側は process_rest の left/right Y 埋めで処理される。
        let data = links(&[("A", "B", 10.0), ("C", "D", 6.0)]);
        let (mut nodes, idx) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        let max_x = calculate_x(&mut nodes, &data, &idx, SankeyModeX::Edge);
        let max_y = calculate_y(&mut nodes, max_x);
        // 全ノードが有限 y を得る。
        for nd in &nodes {
            assert!(nd.y.is_some_and(|y| y.is_finite()), "node {} y", nd.key);
        }
        assert!(max_y.is_finite() && max_y > 0.0);
        // 各列で重なりなし。
        for x in 0..=max_x {
            let mut col: Vec<usize> = (0..nodes.len())
                .filter(|&i| nodes[i].x == Some(x))
                .collect();
            col.sort_by(|&a, &b| {
                nodes[a]
                    .y
                    .unwrap()
                    .partial_cmp(&nodes[b].y.unwrap())
                    .unwrap()
            });
            for w in col.windows(2) {
                let bottom = nodes[w[0]].y.unwrap() + nodes[w[0]].size;
                assert!(
                    bottom <= nodes[w[1]].y.unwrap() + 1e-6,
                    "overlap in column {x}"
                );
            }
        }
    }

    // ── Task 3.4: priority 順 ──

    #[test]
    fn priority_orders_column_ascending() {
        // A,B,C → D(全て列0)。priority C<B<A → y 昇順 C,B,A。
        let data = links(&[("A", "D", 1.0), ("B", "D", 1.0), ("C", "D", 1.0)]);
        let mut prio = HashMap::new();
        prio.insert("A".to_string(), 2.0);
        prio.insert("B".to_string(), 1.0);
        prio.insert("C".to_string(), 0.0);
        let (mut nodes, idx) = build_nodes(&data, SankeySize::Max, &prio, &hm_u());
        let max_x = calculate_x(&mut nodes, &data, &idx, SankeyModeX::Edge);
        let _ = calculate_y_using_priority(&mut nodes, max_x);
        assert!(nodes[idx["C"]].y.unwrap() < nodes[idx["B"]].y.unwrap());
        assert!(nodes[idx["B"]].y.unwrap() < nodes[idx["A"]].y.unwrap());
    }

    #[test]
    fn priority_next_y_start_carries_spanning_flow() {
        // A→B→C に加え列をまたぐ A→C。A=col0, B=col1, C=col2。
        // col0 先頭 A の to のうち x>1 を飛び越す A→C(flow 2)が次列 col1 の起点 y に繰り越される。
        let data = links(&[("A", "B", 3.0), ("B", "C", 3.0), ("A", "C", 2.0)]);
        let mut prio = HashMap::new();
        prio.insert("A".to_string(), 0.0); // priority レイアウトを有効化。
        let (mut nodes, idx) = build_nodes(&data, SankeySize::Max, &prio, &hm_u());
        let max_x = calculate_x(&mut nodes, &data, &idx, SankeyModeX::Edge);
        assert_eq!(nodes[idx["A"]].x, Some(0));
        assert_eq!(nodes[idx["B"]].x, Some(1));
        assert_eq!(nodes[idx["C"]].x, Some(2));
        let _ = calculate_y_using_priority(&mut nodes, max_x);
        // 繰り越しにより col1 の B は y=0 ではなく A→C の flow(2)から始まる。
        assert_eq!(nodes[idx["A"]].y, Some(0.0));
        assert!(
            (nodes[idx["B"]].y.unwrap() - 2.0).abs() < 1e-9,
            "B.y should carry spanning flow 2, got {}",
            nodes[idx["B"]].y.unwrap()
        );
        // col2 の C は繰り越しなし(B の to は x>2 を飛び越さない)→ y=0。
        assert_eq!(nodes[idx["C"]].y, Some(0.0));
    }

    // ── Task 3.5: padding と flow オフセット ──

    #[test]
    fn add_padding_increases_max_y_for_multi_row_column() {
        let data = links(&[
            ("Coal", "Electricity", 25.0),
            ("Gas", "Electricity", 15.0),
            ("Electricity", "Residential", 20.0),
            ("Electricity", "Industrial", 20.0),
        ]);
        let (mut nodes, idx) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        let max_x = calculate_x(&mut nodes, &data, &idx, SankeyModeX::Edge);
        let max_y = calculate_y(&mut nodes, max_x);
        let padding = (max_y / 450.0) * 10.0;
        assert!(padding > 0.0);
        let max_y_padded = add_padding(&mut nodes, padding);
        // padding 挿入で列内に隙間ができ、全体高さが増える。
        assert!(
            max_y_padded > max_y,
            "padded {max_y_padded} should exceed {max_y}"
        );
    }

    #[test]
    fn sort_flows_accumulates_add_y_non_overlapping() {
        // A→C, B→C(C は 2 入力)。非 overlap(size>=in)なら add_y は累積 flow。
        let data = links(&[("A", "C", 4.0), ("B", "C", 6.0)]);
        let (mut nodes, idx) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        let max_x = calculate_x(&mut nodes, &data, &idx, SankeyModeX::Edge);
        let _ = calculate_y(&mut nodes, max_x);
        sort_flows(&mut nodes);
        let c = &nodes[idx["C"]];
        // 先頭 add_y=0、次は先頭 flow ぶん。
        let mut acc = 0.0;
        for e in &c.from {
            assert!((e.add_y - acc).abs() < 1e-9, "add_y {} != {}", e.add_y, acc);
            acc += e.flow;
        }
    }

    // ── レイアウト全体が有限値を返す(NaN ガード) ──

    #[test]
    fn layout_produces_finite_dimensions() {
        let data = links(&[
            ("Coal", "Electricity", 25.0),
            ("Gas", "Electricity", 15.0),
            ("Electricity", "Residential", 20.0),
            ("Electricity", "Industrial", 20.0),
        ]);
        let (mut nodes, idx) = build_nodes(&data, SankeySize::Max, &hm_f(), &hm_u());
        let (max_x, max_y) = layout(
            &mut nodes,
            &data,
            &idx,
            false,
            450.0,
            10.0,
            SankeyModeX::Edge,
        );
        assert!(max_y.is_finite(), "max_y must be finite");
        assert!(max_x <= nodes.len());
        for nd in &nodes {
            assert!(nd.x.is_some(), "every node has x");
            assert!(nd.y.unwrap().is_finite(), "every node y finite");
            for e in nd.from.iter().chain(nd.to.iter()) {
                assert!(e.add_y.is_finite(), "every add_y finite");
            }
        }
    }

    /// テスト用に data から key→index マップを再構築するヘルパ。
    struct SankeyKeyIdx;
    impl SankeyKeyIdx {
        fn idx(data: &[SankeyLink]) -> HashMap<String, usize> {
            let (_, idx) = build_nodes(data, SankeySize::Max, &HashMap::new(), &HashMap::new());
            idx
        }
    }
}
