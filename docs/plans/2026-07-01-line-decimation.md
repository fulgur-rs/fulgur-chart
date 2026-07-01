# 大データ line のデシメーション（Chart.js 互換 decimation プラグイン）

- Issue: fulgur-chart-43h
- 日付: 2026-07-01
- ステータス: 設計合意済み（実装前）

## 目的

巨大な line / area チャートを間引き（decimation）して SVG・PNG 両方を高速化し、
**AI が初手で「いい感じ」のグラフを生成できる**ことを最優先にする。

`options.plugins.decimation` を Chart.js 互換で追加するが、**既定挙動は Chart.js から
意図的に乖離させる**（後述）。

## 背景・動機

- `fulgur-chart-4pn`（stamp cache, PR#102）でマーカー fill は高速化したが、`line_large`
  の残差はポリラインストローク（~30ms、78.9→63.6ms で頭打ち）。
- 動機ケース `line_large` ベンチは **category 軸**（`{"type":"line","data":{"labels":[…1万…],
  "datasets":[{"data":[…1万…]}]}}`）。
- Chart.js の decimation は **x 軸が linear/time のときのみ**発動するため、厳密模倣だと
  この遅いケース（category 軸）に効かない。
- QuickChart は decimation を使っていない（主力が Chart.js v2 ベースで decimation 自体が無い）。
  ただし QuickChart は AI 生成を主用途にしておらず、本プロジェクトのゴール（初手品質）には
  当てはまらない。

## 設計判断（確定事項）

| # | 論点 | 決定 | 備考 |
|---|---|---|---|
| 1 | 適用軸 | **category も対象**（linear/time に限定しない） | Chart.js の軸制約のみ緩和 |
| 2 | 間引きの段 | **データ段（IR/layout）** | SVG・PNG 両方が同一幾何で高速化・一貫 |
| 3 | アルゴリズム | **min-max（既定）と lttb の両方** | 名前・オプションは Chart.js 模倣 |
| 4 | 既定挙動 | **自動オン（threshold ゲート）** | `enabled` 既定 = true。Chart.js（false）から乖離 |
| 5 | 間引き対象 | **line / area / marker / label すべて** | Chart.js の `dataset.data` 差し替えモデル |
| 6 | 巨大 line のマーカー | **threshold 超えで自動抑制** | 間引きだけではマーカー帯が残るため |

### なぜ自動オン（#4）か

AI（LLM）は `decimation.enabled` を自発的に立てない。`enabled:false`（Chart.js 既定）の
ままだと、AI が 1万点 line を投げた初手は「遅い＋マーカー密集の汚い帯」のまま出る。
「初手でいい感じ」というゴールに反する。

Chart.js は元々 `threshold`（既定 = プロット幅px × 4）超過時だけ間引く設計。この threshold
ゲートはそのまま使い、**マスタースイッチ `enabled` の既定だけ true に倒す**。結果：

- threshold 未満（小〜通常サイズ）→ 何も起きない → **今日とバイト不変**。
- threshold 超過（巨大）→ **自動で間引かれ初手から速い**。
- 嫌なら `decimation:{enabled:false}` で明示的に切れる／`algorithm`・`samples`・`threshold`
  で調整可能。

### なぜマーカー抑制（#6）が必要か

min-max は占有ピクセル列ごとに最大4点（start/min/max/end）を残す。マーカーは半径3
（直径6px）で列間隔1pxのため、**隣接列のマーカーが重なり min/max エンベロープをなぞる
約6px厚のリボンになる**。このリボンの輪郭は全点描画でも min/max だけでもほぼ同じ。

→ decimation は「線の高速化＋線のクリーン化」には効くが、**マーカー帯という最大の
見た目の汚さは消えない**。「初手で綺麗」を達成する唯一のレバーがマーカー抑制。

- threshold 超過の line では既定の半径3マーカーを非表示にし、クリーンな線だけにする。
- ユーザーが `pointRadius` を明示指定した場合は尊重（逃げ道）。間引き後の点に描画。
- threshold 未満では従来どおり（マーカー描画・バイト不変）。
- 注: 現状 line は `pointRadius` を無視し `MARKER_R=3` 固定。`pointRadius` の尊重は
  **threshold 超過の経路のみ**に限定する（未満の経路を変えると既存出力が壊れるため）。

## アルゴリズム

両アルゴリズムとも**論理ピクセル空間**（`frame.xs/ys` 適用後）で動作する。`Frame.plot_left/
plot_right` は論理座標（scale は raster で後付け）で、SVG・PNG が同一 frame を共有するため、
論理プロット幅で列バケツを切れば SVG↔PNG の間引きが一致＝決定性が保たれる。

### threshold 判定

点数は**系列全体の合計**で判定（Chart.js セマンティクス）。超えたら gap でセグメント分割
した後、**各セグメントを間引く**。判定をセグメント分割前の系列全体で行うことで、gap で
多数の小セグメントに割れても各セグメントが threshold 未満となり間引きが逃げる事故を防ぐ。

- `threshold` 既定 = 論理プロット幅px × 4
- `samples`（lttb 用）既定 = 論理プロット幅px（1px=1サンプル目安）

### min-max（既定）

`floor(px_x)` を列キーにバケツ化。各占有列で最大4点を x 順に残す：

1. start（列の先頭点）
2. min（最小 y の点）と max（最大 y の点）を index 順に
3. end（列の末尾点）

Chart.js `minMaxDecimation` 準拠（min/max は列平均xに配置、start/end は自身のx、重複除去）。
ピークが保たれる。

### lttb（Largest Triangle Three Buckets）

1. `bucketWidth = (count-2)/(samples-2)`、先頭点を保持。
2. `samples-2` 回反復：候補バケツの各点と「直前採用点・次バケツ平均点」が作る三角形の
   面積を計算、最大の点を採用。
3. 末尾点を保持。

`count ≤ samples` なら間引かず原データ返却。px 空間で面積を取るため視覚的に正しい
（data 空間の Chart.js より歪みが少ない）。

## データフロー

```
系列全体の点数 > threshold?
  ├ No  → 今日と完全に同じ経路（バイト不変）
  └ Yes → gap でセグメント分割 → 各セグメントを間引き
          → line / area は間引き後の点で構築
          → マーカーは抑制（pointRadius 明示時のみ間引き後の点に描画）
          → label も間引き後の点に追従
```

## オプション（Chart.js 互換）

`options.plugins.decimation`:

| キー | 型 | 既定 | 備考 |
|---|---|---|---|
| `enabled` | bool | **`true`**（Chart.js は false） | 本プロジェクトの意図的乖離 |
| `algorithm` | `'min-max'` \| `'lttb'` | `'min-max'` | Chart.js と同じ |
| `samples` | number | 論理プロット幅px | lttb 用、Chart.js と同じ |
| `threshold` | number | 論理プロット幅px × 4 | Chart.js と同じ |

- **schema 型**と**strict パーサの許可キー**を**同時に**追加（fulgur-chart-27k 系の
  parity gap を作らないため）。
- tunable 定数（`STAMP_*` と同様）として既定値を公開。

## エッジケース

- `count ≤ threshold`（lttb は `count ≤ samples`）→ 間引かず原データ返却（バイト不変）。
- `tension > 0`（spline）→ 先に間引き、間引き後の点に catmull-rom 適用（Chart.js も tension
  不問で間引く）。エンベロープ厳密保存は崩れるが許容（実運用は直線が主）。
- **前提条件**: x が index 単調（category/linear スコープでは常に真）。違反時も決定的な
  出力にはなる（意味は薄れる）。
- gap はセグメント分割後に間引くのでバケツをまたがない。
- 複数 dataset は個別に間引く。

## 互換性の乖離（重要・要明記）

fulgur-chart は巨大 line を**既定で自動間引き＋マーカー抑制**する。decimation キー無しの
Chart.js config が **Chart.js と異なる出力**になる（Chart.js 既定は `enabled:false`）。
Chart.js から移植する利用者にとっての footgun。

- 逃げ道: `plugins.decimation.enabled=false`（間引き無効）＋ `pointRadius` 明示（マーカー復活）。
- CHANGELOG / 移行メモに「意図的乖離 + 逃げ道」を明記。

## テスト

- 既存 line golden（小規模・threshold 未満）→ 不変、回帰として維持。
  （`render_line__*`・`golden/line.png` は全て threshold 未満で影響なしを確認済み。
  `line_large` は bench 専用でピクセル golden 無し → 既存 golden 破壊ゼロ。）
- **no-op 証明**: 巨大データ + `enabled:false` == 変更前バイト（passthrough に reorder/avgX
  漏れが無いことを保証）。
- 新規 golden: 間引き後の巨大 line（min-max / lttb、SVG・PNG）。
- SVG↔PNG 間引き一致 / 決定性（同入力 → 同バイト）。
- schema↔strict parser parity テスト（`decimation` キー）。
- bench: `line_large` の decimation-on 版を追加し高速化を実証
  （既存 `line_large` は off-path ベースラインとして維持）。
- **実物の目視確認**（受け入れ条件）: 間引き後 line_large をレンダし、マーカー抑制で
  帯が消えクリーンな線になったか目で確認。「綺麗」を思い込みで断定しない。

## 受け入れ条件

1. `enabled:false` 時、巨大データでも変更前とバイト一致。
2. 既定（自動オン）で巨大 line が間引かれ、SVG・PNG が一致した決定的出力になる。
3. min-max / lttb 両方が動作し、`algorithm` で切り替え可能。
4. threshold 超過の line でマーカーが抑制され、目視でクリーンな線になる。
5. schema と strict parser が `decimation` キーで一致（parity テスト緑）。
6. `line_large` の decimation-on bench で明確な高速化。
7. 全テスト緑、CHANGELOG に互換性乖離を記載。
