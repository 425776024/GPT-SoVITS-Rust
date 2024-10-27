## 介绍

- GPT-SoVITS`纯Rust编译`推理代码（Demo工程代码，未完善）：包含中英文解析、音频输出、音频播放
- 一些字典依赖数据已经输出到data目录，
- onnx导出(对原模型结构进行的onnx适配修改)：python sovits_infer/export_onnx.py 
- data文件目录参考百度网盘: https://pan.baidu.com/s/1-aBsaIaisO_DLy-xZuh7ag?pwd=nei8
- - 更新网盘数据: 'sovits_infer' onnx模型导出工程、依赖的Cargo.toml lib: （部分二次开发了）grapheme_to_phoneme-0.1.0、pinyin-0.10.0、soundtouch-0.4.0

## Run demo
- 1.安装rust
- 2.cargo build or cat `main.rs`

## 讨论
- GPT-SoVITS效果时好时坏，不太稳定，但是作为`Zero-shot voice conversion (5s) / few-shot voice conversion (1min). `个人使用还是不错的

## bilibili
【最近爆火的开源语音克隆项目GPT-SoVITS，我用一个月左右时间把他从原来玩具级别Python工程，用纯高性能Rust编程语言实现了，更快更好更省资源】 https://www.bilibili.com/video/BV11H4y1s7q4/?share_source=copy_web&vd_source=94d735f74b7dcc93cb96880af1582df1
