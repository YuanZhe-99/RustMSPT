# `version` —— 这个二进制是什么

## 功能说明

报告本次构建：包版本、git 提交、构建脚本最后一次运行时工作树是否有改动、启用的 cargo 特性，以及
构建平台。`rustmspt --version` 打印一行，`rustmspt version --json` 以 JSON 对象给出同样的事实。

它之所以存在，是因为把本工具作为外部工具驱动的程序必须记录"是哪一份字节产出了每一件产物"。在它出现
之前，唯一的回答方式是从外部去哈希二进制、并读取检出目录的 `HEAD`。

## 运行方式

```bash
./target/release/rustmspt --version
```

```bash
./target/release/rustmspt version --json
```

## 预期输出

取自一次 release 构建的真实输出：

```
rustmspt 0.2.0 (git 77642fd, dirty; features: default)
```

```json
{
  "name": "rustmspt",
  "version": "0.2.0",
  "git_commit": "77642fdae99098cf810984f3b5086d469a721e8e",
  "git_dirty": true,
  "features": [
    "default"
  ],
  "target": "aarch64-unknown-linux-gnu",
  "host": "aarch64-unknown-linux-gnu",
  "profile": "release",
  "source_date_epoch": null
}
```

`--version` 与 `version` 打印完全相同的文本：两者都是 `build_identity().version_line()`。

## 各字段的含义，以及它们不主张什么

| 字段 | 含义 |
|---|---|
| `version` | 编译期读取的 `Cargo.toml` 包版本。 |
| `git_commit` | 构建所依据的完整 40 字符提交，或 `null`。 |
| `git_dirty` | `build.rs` 最后一次运行时工作树是否有未提交改动。 |
| `features` | 已启用的 cargo 特性，已排序。出现 `default` 是因为 crate 声明了该特性。 |
| `target` / `host` | 编译目标，以及执行编译的机器。 |
| `profile` | `debug` 或 `release`。 |
| `source_date_epoch` | 为可重现构建设置的 `SOURCE_DATE_EPOCH`。 |

有两条性质是刻意为之的。

**没有 git 的检出仍然能构建，也仍然能回答。** 所有 git 调用都做了包装：缺少 git 二进制、没有 `.git`、
或仓库没有任何提交，三种情况都得到 `null` 而非失败。不会 panic，也不会输出构建警告。

**`null` 不等于 `false`。** "无法确定工作树是否有改动"与"工作树是干净的"是两种不同的断言，因此
`git_dirty` 可为空，且只有在同时能给出提交号时才报告干净与否。把这些写入产物的使用方，必须能区分二者。

诚实性的边界值得说明：`git_dirty` 描述的是 `build.rs` 最后一次运行时的工作树状态，可能早于最后一次
编译之后所做的修改。构建脚本会在 `build.rs`、`Cargo.toml`、`Cargo.lock`、`src/` 或 git 引用发生变化时
重跑，从而缩小这个窗口，但无法将其完全消除。

## 说明

- `--help` 的形态未变：第一行仍是程序说明，`Usage:` 仍在第三行。新增 `version` 只是在子命令列表里
  多了一行，别无其他影响。
- 同一个身份对象会作为 `tool` 嵌入放置记录与运行报告，因此产物自带产出它的那次构建的信息。
- `BuildIdentity`、`build_identity`，以及负责写入这些值的 `build.rs` 各函数，见
  [`../reference/core-and-compute.md`](../reference/core-and-compute.md)。
