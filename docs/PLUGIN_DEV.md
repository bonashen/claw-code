# Claw 插件开发手册

 claw 的插件系统允许扩展 CLI 功能，提供自定义工具、命令和生命周期钩子。

## 目录

- [插件概述](#插件概述)
- [插件结构](#插件结构)
- [manifest.json 规范](#manifestjson-规范)
- [工具定义](#工具定义)
- [命令定义](#命令定义)
- [钩子系统](#钩子系统)
- [生命周期](#生命周期)
- [权限系统](#权限系统)
- [安装与注册](#安装与注册)

---

## 插件概述

### 插件类型

```rust
pub enum PluginKind {
    Builtin,   // 内置插件
    Bundled,    // 打包插件
    External,  // 外部插件
}
```

| 类型 | 说明 |
|------|------|
| `Builtin` | 随 CLI 内置 |
| `Bundled` | 预打包可分发 |
| `External` | 用户安装的外部插件 |

---

## 插件结构

```
my-plugin/
├── .claude-plugin/
│   └── plugin.json    # 插件清单 (必需)
├── scripts/         # 可选脚本目录
└── README.md        # 可选文档
```

### 插件清单 (plugin.json)

```json
{
  "name": "my-plugin",
  "version": "1.0.0",
  "description": "我的插件描述",
  "permissions": ["read", "write", "execute"],
  "defaultEnabled": true,
  "hooks": {
    "PreToolUse": ["scripts/pre_tool.sh"],
    "PostToolUse": ["scripts/post_tool.sh"],
    "PostToolUseFailure": ["scripts/failure.sh"]
  },
  "lifecycle": {
    "Init": ["scripts/init.sh"],
    "Shutdown": ["scripts/shutdown.sh"]
  },
  "tools": [
    {
      "name": "my-tool",
      "description": "我的自定义工具",
      "inputSchema": {
        "type": "object",
        "properties": {
          "arg": { "type": "string" }
        }
      },
      "command": "python",
      "args": ["scripts/my_tool.py"],
      "requiredPermission": "write"
    }
  ],
  "commands": [
    {
      "name": "/my-command",
      "description": "我的 slash 命令"
    }
  ]
}
```

---

## manifest.json 规范

### 基本字段

| 字段 | 类型 | 必需 | 说明 |
|------|------|------|------|
| `name` | string | 是 | 插件唯一标识 |
| `version` | string | 是 | 语义化版本 |
| `description` | string | 是 | 描述文本 |

### 权限

```rust
pub enum PluginPermission {
    Read,      // 读权限
    Write,     // 写权限
    Execute,   // 执行权限
}
```

### 钩子配置

```json
{
  "hooks": {
    "PreToolUse": ["path/to/script"],
    "PostToolUse": ["path/to/script"],
    "PostToolUseFailure": ["path/to/script"]
  }
}
```

### 生命周期配置

```json
{
  "lifecycle": {
    "Init": ["path/to/init"],
    "Shutdown": ["path/to/shutdown"]
  }
}
```

---

## 工具定义

### 工具清单 (PluginToolManifest)

```json
{
  "name": "tool-name",
  "description": "工具描述",
  "inputSchema": {
    "type": "object",
    "properties": {
      "param1": { "type": "string" },
      "param2": { "type": "number" }
    },
    "required": ["param1"]
  },
  "command": "python",
  "args": ["scripts/tool.py"],
  "requiredPermission": "write"
}
```

### 工具权限级别

```rust
pub enum PluginToolPermission {
    ReadOnly,        // 只读
    WorkspaceWrite,  // 工作区写
    DangerFullAccess, // 完全访问
}
```

| 级别 | 说明 |
|------|------|
| `read-only` | 仅读取文件 |
| `workspace-write` | 可写入工作区 |
| `danger-full-access` | 完全访问权限 |

### 工具执行

工具通过命令行执行，接收 JSON 输入：

```python
# scripts/tool.py
import sys
import json

def main():
    input_data = json.load(sys.stdin)
    # 处理输入
    result = {"output": "result"}
    print(json.dumps(result))

if __name__ == "__main__":
    main()
```

---

## 命令定义

### 命令清单 (PluginCommandManifest)

```json
{
  "commands": [
    {
      "name": "/my-command",
      "description": "命令描述"
    }
  ]
}
```

命令在 REPL 中以 `/` 前缀调用。

---

## 钩子系统

### 钩子类型

```rust
pub enum HookEvent {
    PreToolUse,           // 工具执行前
    PostToolUse,          // 工具执行后
    PostToolUseFailure,  // 工具执行失败后
}
```

### 钩子脚本

钩子脚本接收环境变量和标准输入：

```bash
#!/bin/bash
# 钩子脚本示例
# 环境变量:
#   TOOL_NAME - 工具名称
#   TOOL_INPUT - 工具输入 (JSON)
#   RESULT - 执行结果 (PostToolUse only)

echo "Tool: $TOOL_NAME"
echo "Input: $TOOL_INPUT"
```

### 钩子返回值

```rust
pub struct HookRunResult {
    denied: bool,      // 是否拒绝执行
    failed: bool,    // 是否失败
    messages: Vec<String>,  // 返回消息
}
```

---

## 生命周期

### Init 钩子

在插件加载时执行，用于初始化：

```json
{
  "lifecycle": {
    "Init": ["scripts/init.sh"]
  }
}
```

### Shutdown 钩子

在 CLI 关闭时执行，用于清理：

```json
{
  "lifecycle": {
    "Shutdown": ["scripts/cleanup.sh"]
  }
}
```

---

## 权限系统

### 插件权限

```rust
pub enum PluginPermission {
    Read,    // 读取文件
    Write,   // 写入文件
    Execute, // 执行命令
}
```

插件清单中声明所需权限：

```json
{
  "permissions": ["read", "write", "execute"]
}
```

### 工具权限

每个工具独立声明权限：

```json
{
  "requiredPermission": "write"
}
```

---

## 安装与注册

### 安装位置

用户插件安装在：

- `$CLAW_CONFIG_HOME/plugins`
- `~/.claw/plugins`

### 注册文件

已安装插件记录在 `installed.json`：

```json
{
  "plugins": [
    {
      "id": "my-plugin",
      "kind": "external",
      "source": "/path/to/plugin",
      "enabled": true
    }
  ]
}
```

### CLI 命令

```bash
# 列出插件
claw plugin list

# 安装插件
claw plugin install /path/to/plugin

# 启用插件
claw plugin enable my-plugin

# 禁用插件
claw plugin disable my-plugin

# 卸载插件
claw plugin uninstall my-plugin
```

---

## 完整示例

### 插件目录结构

```
my-awesome-plugin/
├── .claude-plugin/
│   └── plugin.json
└── scripts/
    └── say_hello.py
```

### plugin.json

```json
{
  "name": "my-awesome-plugin",
  "version": "1.0.0",
  "description": "一个很棒的示例插件",
  "permissions": ["read", "execute"],
  "defaultEnabled": true,
  "hooks": {
    "PreToolUse": ["scripts/check.sh"]
  },
  "lifecycle": {
    "Init": ["scripts/init.sh"]
  },
  "tools": [
    {
      "name": "hello",
      "description": "打印问候信息",
      "inputSchema": {
        "type": "object",
        "properties": {
          "name": {
            "type": "string",
            "description": "名字"
          }
        }
      },
      "command": "python",
      "args": ["scripts/say_hello.py"],
      "requiredPermission": "read-only"
    }
  ],
  "commands": [
    {
      "name": "/hello",
      "description": "打印问候信息"
    }
  ]
}
```

### 工具脚本

```python
#!/usr/bin/env python3
import sys
import json

def main():
    data = json.load(sys.stdin)
    name = data.get("name", "World")
    print(json.dumps({"output": f"Hello, {name}!"}))

if __name__ == "__main__":
    main()
```

### 安装

```bash
claw plugin install ./my-awesome-plugin
claw plugin enable my-awesome-plugin
claw /hello
```

---

## API 参考

### 核心类型

```rust
// 插件元数据
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub kind: PluginKind,
    pub source: String,
    pub default_enabled: bool,
    pub root: Option<PathBuf>,
}

// 插件清单
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: Vec<PluginPermission>,
    pub default_enabled: bool,
    pub hooks: PluginHooks,
    pub lifecycle: PluginLifecycle,
    pub tools: Vec<PluginToolManifest>,
    pub commands: Vec<PluginCommandManifest>,
}
```

### 错误类型

```rust
pub enum PluginError {
    NotFound(String),
    InvalidManifest(String),
    LoadError(String),
    ExecutionError(String),
}
```