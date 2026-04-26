# Claw 配置文件模板 (.claw.json)

本文档提供 `.claw.json` 配置文件的完整模板和说明。

## 配置文件位置

配置文件按以下优先级合并（后面的覆盖前面的）：

1. `~/.claw.json` - 用户全局配置
2. `~/.claw/settings.json` - 用户配置
3. `{cwd}/.claw.json` - 项目配置
4. `{cwd}/.claw/settings.json` - 项目配置
5. `{cwd}/.claw/settings.local.json` - 本地配置（不提交到版本控制）

## 完整配置模板

```json
{
  "$schema": "https://raw.githubusercontent.com/ultraworkers/claw-code/main/schema/claw-settings.schema.json",
  "model": "claude-sonnet-4-6",
  "env": {
    "ANTHROPIC_API_KEY": "sk-ant-...",
    "OPENAI_API_KEY": "sk-..."
  },
  "permissionMode": "acceptEdits",
  "permissions": {
    "allow": [
      "Read",
      "Glob",
      "Grep",
      "Write",
      "Edit",
      "Bash(npm *)",
      "Bash(cargo *)"
    ],
    "deny": [
      "Bash(rm -rf /)",
      "Bash(sudo *)",
      "Bash(curl *|sh)"
    ],
    "ask": [
      "Edit",
      "Write",
      "Bash(git push)",
      "Bash(rm -rf *)"
    ]
  },
  "hooks": {
    "PreToolUse": [],
    "PostToolUse": [],
    "PostToolUseFailure": []
  },
  "sandbox": {
    "enabled": false,
    "namespaceRestrictions": false,
    "networkIsolation": false,
    "filesystemMode": "read-write",
    "allowedMounts": ["/tmp", "/home"]
  },
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home"]
    },
    "brave-search": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-brave-search"],
      "env": {
        "BRAVE_API_KEY": "your-brave-api-key"
      }
    },
    "remote-server": {
      "url": "https://mcp.example.com/sse",
      "headers": {
        "Authorization": "Bearer token"
      }
    }
  },
  "plugins": {
    "enabled": true,
    "externalDirectories": [],
    "installRoot": null,
    "registryPath": null,
    "bundledRoot": null,
    "maxOutputTokens": 8192
  },
  "providerFallbacks": {
    "primary": "claude-sonnet-4-6",
    "fallbacks": ["claude-haiku-4-5-20251213", "gpt-4o"]
  },
  "aliases": {
    "opus": "claude-opus-4-6",
    "sonnet": "claude-sonnet-4-6",
    "haiku": "claude-haiku-4-5-20251213"
  },
  "trustedRoots": [
    "/usr/local/bin",
    "/home/.local/bin"
  ]
}
```

## 字段详解

### model

```json
"model": "claude-sonnet-4-6"
```

指定默认使用的模型。可用模型别名：
- `opus` → `claude-opus-4-6`
- `sonnet` → `claude-sonnet-4-6`  
- `haiku` → `claude-haiku-4-5-20251213`

### env

```json
"env": {
  "ANTHROPIC_API_KEY": "sk-ant-...",
  "OPENAI_API_KEY": "sk-...",
  "ANTHROPIC_BASE_URL": "https://your-proxy.com"
}
```

环境变量，注入到工具执行环境。

### permissionMode

```json
"permissionMode": "acceptEdits"
```

权限模式：
- `default` / `plan` / `read-only` - 只读模式
- `acceptEdits` / `auto` / `workspace-write` - 可编辑模式
- `dontAsk` / `danger-full-access` - 完全信任模式

### permissions

```json
"permissions": {
  "allow": ["Read", "Write", "Bash(npm *)"],
  "deny": ["Bash(rm -rf /)"],
  "ask": ["Edit", "Write"]
}
```

| 字段 | 说明 |
|------|------|
| `allow` | 自动允许的工具 |
| `deny` | 自动拒绝的工具 |
| `ask` | 需要用户确认的工具 |

### hooks

```json
"hooks": {
  "PreToolUse": ["/path/to/hook"],
  "PostToolUse": ["/path/to/hook"],
  "PostToolUseFailure": ["/path/to/hook"]
}
```

生命周期钩子脚本路径。

### sandbox

```json
"sandbox": {
  "enabled": true,
  "namespaceRestrictions": true,
  "networkIsolation": true,
  "filesystemMode": "read-write",
  "allowedMounts": ["/tmp"]
}
```

沙箱配置：

| 字段 | 类型 | 说明 |
|------|------|------|
| `enabled` | boolean | 启用沙箱 |
| `namespaceRestrictions` | boolean | 命名空间隔离 |
| `networkIsolation` | boolean | 网络隔离 |
| `filesystemMode` | string | `read-only`, `read-write`, `deny-all` |
| `allowedMounts` | array | 允许挂载的目录 |

### mcpServers

```json
"mcpServers": {
  "my-server": {
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home"],
    "env": {
      "KEY": "value"
    }
  }
}
```

MCP 服务器配置，支持多种传输方式：

#### Stdio (本地进程)

```json
"server-name": {
  "command": "node",
  "args": ["/path/to/server.js"],
  "env": {},
  "toolCallTimeoutMs": 30000
}
```

#### HTTP/SSE (远程)

```json
"server-name": {
  "url": "https://mcp.example.com/sse",
  "headers": {
    "Authorization": "Bearer token"
  }
}
```

#### WebSocket

```json
"server-name": {
  "url": "wss://mcp.example.com/ws"
}
```

### plugins

```json
"plugins": {
  "enabled": true,
  "externalDirectories": ["/path/to/plugins"],
  "installRoot": "~/.claw/plugins",
  "maxOutputTokens": 8192
}
```

插件配置：

| 字段 | 类型 | 说明 |
|------|------|------|
| `enabled` | boolean | 启用插件 |
| `externalDirectories` | array | 外部插件目录 |
| `installRoot` | string | 插件安装根目录 |
| `maxOutputTokens` | number | 插件输出最大 token 数 |

### providerFallbacks

```json
"providerFallbacks": {
  "primary": "claude-sonnet-4-6",
  "fallbacks": ["claude-haiku-4-5-20251213", "gpt-4o"]
}
```

模型降级配置。当前一个模型失败（429/500/503）时自动尝试下一个。

### aliases

```json
"aliases": {
  "my-model": "claude-sonnet-4-6"
}
```

模型别名映射。

### trustedRoots

```json
"trustedRoots": [
  "/usr/local/bin",
  "/home/.local/bin"
]
```

可信路径列表，用于安全检查。

## 最小配置

```json
{
  "model": "sonnet"
}
```

## 推荐的项目配置

在项目根目录创建 `.claw/settings.local.json`（加入 .gitignore）：

```json
{
  "model": "sonnet",
  "permissionMode": "acceptEdits",
  "mcpServers": {}
}
```

在项目根目录创建 `.claw.json`（可提交）：

```json
{
  "model": "sonnet",
  "aliases": {
    "opus": "claude-opus-4-6",
    "sonnet": "claude-sonnet-4-6"
  }
}
```