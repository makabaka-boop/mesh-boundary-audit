# meshcheck — 三角面片组合拓扑审计器

一个零依赖的 Rust 命令行工具，读取**顶点 id** 与**有向三角面**列表，
证明输入是否构成一张可靠的**组合拓扑**（combinatorial）三角曲面。

> ⚠️ 本工具只证明**组合拓扑**性质：边配对、顶点链接、整体连通。
> 它不读取任何坐标，**不检测几何自交、退化嵌入或翻转法线的几何表现**。

## 两种审计模式

- **封闭模式（默认）**：要求输入是水密的**可定向封闭**三角曲面。
- **带边界模式（`-b` / `--boundary`）**：为三维扫描"预留孔口、后续封盖"
  的网格设计。允许一条边恰被一个面使用（合法孔边），成功时列出由这些
  单面边诱导出的**互不重叠的边界环**，并按 χ = 2 − 2g − b 求亏格。

  - 单面边按其唯一面的方向行走；各边界顶点的链接是一条简单路径，故
    单面边恰好分成互不相交的有向环。
  - 两面共边仍**必须反向**；三面以上、同向两面依旧按原边级优先级失败。
  - 每个顶点的链接只能是**一条简单路径（边界顶点）或一个环（内部顶点）**；
    分叉、双扇区、孤立顶点依旧失败。
  - 整体仍须**按共边连通**（仅顶点相碰不算连通）。
  - 每个边界环从其**最小顶点 id**开始（只旋转、**不反转**诱导方向），
    各环按首顶点排序。
  - 只有当 b 与 χ 使 2g = 2 − b − χ 为非负偶数时才给出成功结论；
    公式不成立时报告 `topology-fail`（退出码 5），不伪称成功。

## 审计的问题

每条无向边恰好出现两次，并不足以保证曲面可靠封闭——两个封闭壳
只在一个顶点上相碰时，所有边依然两两反向配对，但该顶点处会
**捏成两个互不相连的扇区**。因此审计按**固定优先级**分三级进行：

1. **边级（edge）**：每条无向边必须恰被两个面使用，且两面以
   **相反方向**经过它；在带边界模式下，单面使用是合法孔边。
   - 仅出现一次 → `boundary`（边界边；仅封闭模式失败）
   - 出现两次但同向 → `misoriented`（翻面；两种模式都失败）
   - 出现 ≥ 3 次 → `nonmanifold`（非流形边；两种模式都失败）
   - 见证（witness）取字典序最小的故障无向边
     `(min(id1,id2), max(id1,id2))`。

2. **顶点级（vertex sectors）**：每个顶点周围的面必须连成唯一一个
   循环扇区，即该顶点的链接（link）是单一有向环。对有向面
   `(x,y,z)`，在顶点 `v` 处的链接边从 `v` 的后继指向前驱；共享边
   把相邻面的链接端点粘起来，链接的弱连通分量数就是 `v` 处的
   扇区数。封闭模式下扇区数必须为 1（双壳共点 = 2，孤立声明顶点 = 0
   均失败）；带边界模式下唯一扇区还可以是一条**简单路径**（恰有两个
   度为 1 的端点），其余结构（分叉、双路径/双扇区、孤立点）依旧失败，
   见证取字典序最小的顶点 id 及其扇区数。

3. **整体连通（component）**：所有面必须处于同一个共边连通分量
   （两种模式相同）。见证为**不含全局最小顶点的分量中字典序最小的
   顶点 id**。

全部通过后输出 `V`、`E`、`F`、欧拉示性数 `χ = V − E + F` 与整数亏格：
封闭模式 `g = (2 − χ)/2`；带边界模式解 `χ = 2 − 2g − b`，其中 `b` 为
边界环数，无合法非负整数解时不得成功。

## 输入格式

UTF-8 ASCII 文本，每行一条记录，`#` 之后为行内注释，空行允许：

```text
v <id>                    # 声明一个顶点
f <id> <id> <id>          # 一个有向三角面（按面的一致绕序书写）
```

- 顶点 id：1–32 个字符，字符集 `[A-Za-z0-9_-]`。
- 规模限定：**4–500** 个唯一顶点，**4–2000** 个面。
- 硬性拒绝（在拓扑审计之前，不进入三级流程）：
  无法识别的行、顶点/面记录字段数错误、非法 id、面内三顶点不互异、
  顶点重复声明、面引用未知顶点、**重复无向面（即使第二份反向书写）**。

## 输出与退出码

```text
ok V=4 E=6 F=4 chi=2 genus=0
ok-boundary V=5 E=8 F=4 chi=1 genus=0 holes=1
boundary a-b-c-d
ok-boundary V=8 E=16 F=8 chi=0 genus=0 holes=2
boundary a-b-c-d
boundary e-h-g-f
edge-fail edge=a-b uses=1 fault=boundary
edge-fail edge=a-b uses=2 fault=misoriented
edge-fail edge=a-b uses=6 fault=nonmanifold
vertex-fail vertex=a sectors=2
component-fail vertex=e
topology-fail V=… E=… F=… chi=… holes=…
reject kind=unknown_vertex line=9 id=x
reject kind=duplicate_face line=11
reject kind=bad_face_arity line=8
```

带边界模式成功时，摘要行之后每个边界环一行 `boundary v0-v1-…`
（首尾不重复）；文本输出中的环与结构化报告 `Report::BoundaryOk`
里的 `boundary` 字段是同一份结果。

| 退出码 | 含义                                 |
| ------ | ------------------------------------ |
| 0      | 曲面合法（封闭或带边界），已给出 V/E/F/χ/亏格（及边界环） |
| 1      | 输入被拒绝（语法/引用/重复面/越界）  |
| 2      | 边级失败                             |
| 3      | 顶点扇区失败（也用于 `--help`/用法错误） |
| 4      | 整体不连通                           |
| 5      | 带边界模式下 χ = 2−2g−b 无非负整数解 |

## 使用

### Cargo

```bash
cargo build --release
cargo test                                        # 证据测试
./target/release/meshcheck examples/torus9.mesh   # 封闭模式（默认）
./target/release/meshcheck -b examples/disc.mesh  # 带边界审计
cat examples/tet.mesh | meshcheck                 # 无参数或 `-` 时读标准输入
```

### Docker Compose

`meshcheck` 服务默认对镜像内置的 9 顶点环面夹具做一次审计：

```bash
docker compose build
docker compose run --rm meshcheck                  # 默认审计 torus9
docker compose run --rm meshcheck /examples/tet.mesh
# 把待审计文件放进 ./work 后：
docker compose run --rm meshcheck /work/your-model.mesh
```

## 夹具（examples/）

| 文件                            | 预期结果                                   |
| ------------------------------- | ------------------------------------------ |
| `tet.mesh`                      | 封闭四面体，χ=2，亏格 0（合法封闭体）      |
| `torus9.mesh`                   | 9 顶点 18 面环面，χ=0，亏格 1（合法）      |
| `genus2.mesh`                   | 两个环面沿洞反向粘合（T#T），χ=−2，亏格 2  |
| `disc.mesh`                     | 三角剖分盘片：1 个边界环 `a-b-c-d`，χ=1    |
| `annulus.mesh`                  | 三角环带：2 个边界环，χ=0，亏格 0          |
| `punctured-torus.mesh`          | torus9 删一面：b=1，χ=−1，亏格 1，环 `0-4-1` |
| `boundary.mesh`                 | 五面体缺一个顶盖 → 封闭模式报边界边 `a-b`；带边界模式为盘片 |
| `torn-aperture.mesh`            | 盘片 + 仅共点的三角片（断裂孔口）→ 顶点 `a` 两个扇区 |
| `flipped.mesh`                  | 翻面 → 最小同向边 `a-b`（两种模式都失败）  |
| `nonmanifold-edge.mesh`         | 三个四面体共一条边 → 非流形边 `a-b`        |
| `two-shells-shared-vertex.mesh` | 两壳共一个顶点 → 顶点 `a` 两个扇区         |
| `disjoint-shells.mesh`          | 两个互不相连的封闭四面体 → 分量见证 `e`    |
| `unknown-vertex.mesh`           | 引用未声明顶点                             |
| `duplicate-face.mesh`           | 无向面重复（第二份反向书写）               |
| `extra-field.mesh`              | 面记录含额外字段                           |

## 限制

- 不读取坐标，因此**不能**报告几何自交、零面积面或法线不一致的
  几何后果；这里的"方向一致"纯由面绕序的配对定义。
- 输出的整数亏格仅在审计全通过（封闭模式：封闭可定向连通三角曲面；
  带边界模式：带 b 个边界环的可定向连通紧曲面）时才有拓扑意义。
  带边界模式下若 χ = 2−2g−b 没有非负整数解，工具以
  `topology-fail`（退出码 5）拒绝成功结论。
