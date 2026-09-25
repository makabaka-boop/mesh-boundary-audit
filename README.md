# meshcheck — 三角面片组合拓扑审计器

一个零依赖的 Rust 命令行工具，读取**顶点 id** 与**有向三角面**列表，
证明输入是否构成一张**组合拓扑水密**（combinatorial watertight）的
可定向封闭三角曲面；使用显式的 `--boundary` 审计模式时，也接受带边界的
紧致可定向三角曲面并列出边界环。

> ⚠️ 本工具只证明**组合拓扑**水密性：边配对、顶点扇区、整体连通。
> 它不读取任何坐标，**不检测几何自交、退化嵌入或翻转法线的几何表现**。

## 审计的问题

工具默认使用**封闭模式**；每条无向边恰出现两次并不足以保证曲面可靠封闭——
两个封闭壳只在一个顶点上相碰时，所有边依然两两反向配对，但该顶点处会
**捏成两个互不相连的扇区**。显式 `--boundary` 模式允许合法孔边：边可恰被
一个面使用；其余流形、顶点链接、整体连通条件仍然必须成立。审计按
**固定优先级**分三级进行：

1. **边级（edge）**：封闭模式下每条无向边必须恰被两个面使用，且两面以
   **相反方向**经过它；边界模式下也允许只使用一次。
   - 仅出现一次：封闭模式 → `boundary`（边界边）；边界模式视为合法单面边
   - 出现两次但同向 → `misoriented`（翻面）
   - 出现 ≥ 3 次 → `nonmanifold`（非流形边）
   - 见证（witness）取字典序最小的故障无向边
     `(min(id1,id2), max(id1,id2))`。

2. **顶点级（vertex links）**：每个顶点周围的面只能组成一个简单结构。
   封闭模式要求唯一一个循环扇区；边界模式要求该顶点链接（link）是
   **唯一一个环或唯一一条简单路径**。对有向面 `(x,y,z)`，在顶点 `v` 处的
   链接边从 `v` 的后继指向前驱；共享边把相邻面的链接端点粘起来，链接的
   弱连通分量数就是 `v` 处的扇区数。分叉、双壳共点（两个扇区）和孤立声明
   顶点（零个扇区）在两种模式下都失败，见证取字典序最小的顶点 id 及其
   扇区数。

3. **整体连通（component）**：所有面必须通过**共享一条边**处于同一个
   连通分量；只共顶点不算共边连通。见证为**不含全局最小顶点的分量中
   字典序最小的顶点 id**。

封闭模式全部通过后输出 `V`、`E`、`F`、欧拉示性数 `χ = V − E + F` 与
整数亏格 `g = (2 − χ)/2`。边界模式通过后，先沿单面边的诱导方向提取互不
重叠的边界环，每环旋转到最小顶点开头且不反转方向，各环再按首顶点排序；
随后输出边界环数 `b` 并求解 `χ = 2 − 2g − b`。只有 `g` 为非负整数时才
报告成功，否则输出不变量失败而不会伪造亏格。

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
boundary-ok V=5 E=9 F=5 chi=1 boundaries=1 genus=0 loop=a,b,t
boundary-ok V=6 E=9 F=6 chi=3 boundaries=2 genus=0 loop=a,b,c loop=d,f,e
edge-fail edge=a-b uses=1 fault=boundary
edge-fail edge=a-b uses=2 fault=misoriented
edge-fail edge=a-b uses=6 fault=nonmanifold
vertex-fail vertex=a sectors=2
component-fail vertex=e
invariant-fail V=... E=... F=... chi=... boundaries=...
reject kind=unknown_vertex line=9 id=x
reject kind=duplicate_face line=11
reject kind=bad_face_arity line=8
```

`loop=` 后的逗号序列是一个边界环，顺序来自单面边方向；多个 `loop=`
之间互不重叠，命令行文本和 Rust 结构化报告使用同一份环结果。

| 退出码 | 含义                                 |
| ------ | ------------------------------------ |
| 0      | 合法曲面，已给出 V/E/F/χ/亏格（边界模式还给出边界环） |
| 1      | 输入被拒绝（语法/引用/重复面/越界）  |
| 2      | 边级失败                             |
| 3      | 顶点链接失败（也用于 `--help`）      |
| 4      | 整体不共边连通                       |
| 5      | `χ = 2−2g−b` 不能得到非负整数亏格    |
| 3      | 用法/IO 错误（多参数、文件不可读等） |

## 使用

### Cargo

```bash
cargo build --release
cargo test                                        # 证据测试
./target/release/meshcheck examples/torus9.mesh   # 默认封闭模式
./target/release/meshcheck --boundary examples/boundary.mesh
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
| `boundary.mesh`                 | 五面体缺一个顶盖 → 边界环 `a,b,t`          |
| `annulus.mesh`                  | 环带：两个边界环，V=6/E=9/F=6，χ=3，g=0    |
| `torn-aperture.mesh`            | 两个盘片只共顶点 `a` → 两个路径链接，失败  |
| `flipped.mesh`                  | 翻面 → 最小同向边 `a-b`                    |
| `nonmanifold-edge.mesh`         | 三个四面体共一条边 → 非流形边 `a-b`        |
| `two-shells-shared-vertex.mesh` | 两壳共一个顶点 → 顶点 `a` 两个扇区         |
| `disjoint-shells.mesh`          | 两个互不相连的封闭四面体 → 分量见证 `e`    |
| `unknown-vertex.mesh`           | 引用未声明顶点                             |
| `duplicate-face.mesh`           | 无向面重复（第二份反向书写）               |
| `extra-field.mesh`              | 面记录含额外字段                           |

## 限制

- 不读取坐标，因此**不能**报告几何自交、零面积面或法线不一致的
  几何后果；这里的“方向一致”纯由面绕序的配对定义。
- 输出的整数亏格仅在所有组合审计通过且 `χ = 2−2g−b` 成立时才有拓扑意义；
  封闭模式 `b=0`。
