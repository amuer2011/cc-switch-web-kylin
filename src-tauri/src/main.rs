// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(not(feature = "desktop"))]
    {
        panic!("Desktop binary 'cc-switch' requires the 'desktop' feature. Use the 'cc-switch-web' binary for web mode.");
    }

    #[cfg(feature = "desktop")]
    {
        // 在 Linux 上设置 WebKit 环境变量以解决 DMA-BUF 渲染问题
        #[cfg(target_os = "linux")]
        {
            if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
                std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            }
            if std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err() {
                std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
            }
        }

        cc_switch_lib::run();
    }
}
