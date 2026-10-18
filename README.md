<!-- Author: Passwd Word -->
# p2p-http-gateway

把 `p2p://IP:19806/<32位hex>`（南京纳加 NagaSoft VJLive / ForceTech ForceTV 私有 P2P 直播协议）
转换成 APTV / 播放器可播的 HTTP(HLS) 链接。带 Web 前端 + Rust 后端，完整一套。

## 架构

```
浏览器/APTV
   │  HTTP
   ▼
Rust 网关 (Axum, 0.0.0.0:8080)
   ├─ Web 前端 /            添加频道、看状态、复制播放地址
   ├─ REST API /api/...     频道增删查、状态、聚合订阅
   ├─ HLS 代理 /live/:id.m3u8  把引擎的 TS 流切成伪 HLS 分片
   └─ 引擎侧车管理            每个频道一个 qemu-arm + libforcetv.so 进程
           │
           ▼
   qemu-arm-static -L <ENGINE_ROOT> launcher libp2p_patched.so \
       -s <server> -c <guid> -o <port>
           │
           ▼  (引擎自带 HTTP 服务)
   http://127.0.0.1:<port>/forcelive&begin=loading&count=0/<guid>.ts
```

## 快速开始

1. 准备引擎目录（ARM 二进制 + shim，见 `~/workspace/p2p-protocol-recon/broot/`）：
   ```
   /opt/p2p-engine/
     qemu-arm-static
     launcher                  # patchelf 加过 DT_NEEDED libbionic_shim.so
     libp2p_patched.so         # patch 掉 forcetv_system_test/std_test 的版本
     lib/
       libc.so libm.so libdl.so libstdc++.so   # ARM glibc 改名伪装 bionic
       libbionic_shim.so       # 自研 bionic 兼容层
   ```

2. 构建运行：
   ```bash
   cargo build --release
   ENGINE_ROOT=/opt/p2p-engine BIND=0.0.0.0:8080 ./target/release/p2p-http-gateway
   ```

3. 浏览器打开 `http://服务器IP:8080/`，粘贴 `p2p://` 链接添加频道，
   复制 `/live/<id>.m3u8` 到 APTV 播放；或直接订阅 `/playlist.m3u`。

## API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/status` | 网关状态 |
| GET/POST | `/api/channels` | 频道列表 / 添加频道 `{"url":"p2p://…","name":"广东体育"}` |
| GET/DELETE | `/api/channels/:id` | 频道详情 / 删除 |
| GET | `/playlist.m3u` | 聚合订阅（APTV 用） |
| GET | `/live/:id.m3u8` | 单频道 HLS 播放清单 |
| GET | `/live/:id/:seg` | TS 分片代理 |

## 当前状态（2026-10-18，诚实说明）

- ✅ 协议逆向完成：URL 语义、UDP 登录包结构、引擎 CLI 与 HTTP 输出格式
- ✅ 引擎可在 x86_64 上经 qemu-arm + bionic shim 运行（`forcetv_print_help` 正常）
- ✅ pthread 死锁已修复（版本符号 `@GLIBC_2.4` 拦截 + 地址映射）
- ❌ **`forcetv_init` 在沙箱环境崩溃（NULL+8）**，引擎侧车无法完成初始化，
  因此当前网关能跑、能管频道，但拉不到真实流。修复引擎初始化后即全通。
- 逆向细节：`~/workspace/p2p-protocol-recon/RECON.md`、`EMULATION.md`

## 后续

1. 修好引擎 `forcetv_init`（或在真 Android 上跑引擎，网关照样管）；
2. 按需把伪 HLS 换成真正分片器（当前 Range 切片 APTV 可播）；
3. 按用户交付规范走 GitHub + Actions 构建镜像（参考 YSP-Yang 仓库模式）。
