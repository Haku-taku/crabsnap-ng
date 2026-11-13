toolbar-title = Crabsnap
toolbar-hint = 抓取尚未接入

# 选区工具栏。
action-save = 保存
action-copy = 复制
action-pin = 贴图
action-cancel = 取消
action-confirm = 确认

# 贴图的右键菜单。
pin-menu-copy = 复制
pin-menu-save = 保存
pin-menu-reset-zoom = 实际大小
pin-menu-edit = 编辑
pin-menu-done = 完成
pin-menu-close = 关闭

# 操作在屏幕上没有别的痕迹时，surface 底部的那条提示。
notice-copied = 已复制到剪贴板
notice-saved = 已保存 { $file }
notice-failed = { $action }失败

# 标注会话的工具与菜单项。
pin-edit-rect = 矩形
pin-edit-ellipse = 椭圆
pin-edit-line = 直线
pin-edit-arrow = 箭头
pin-edit-pen = 自由画笔
pin-edit-bezier = 贝塞尔钢笔
pin-edit-text = 文字
pin-edit-edit-text = 编辑文字
pin-edit-edit-points = 编辑节点
pin-edit-fill = 填充
pin-edit-delete = 删除
pin-edit-finish-text = 完成文字
pin-edit-discard-text = 放弃修改
pin-edit-finish-path = 完成路径
pin-edit-discard-path = 放弃路径
toolbar-cursor = 光标
toolbar-loupe = 放大镜
toolbar-loupe-temporary = 放大镜（右键临时呼出）
toolbar-stroke = 描边
toolbar-colour = 颜色
toolbar-width = 粗细
toolbar-heads = 箭头端点
toolbar-head-end = 末端
toolbar-head-both = 两端
toolbar-outline = 文字描边
toolbar-border = 边框
toolbar-size = 字号
toolbar-padding = 内边距
toolbar-radius = 圆角
toolbar-align = 对齐
toolbar-align-left = 左对齐
toolbar-align-center = 居中
toolbar-align-right = 右对齐
toolbar-none = 无
toolbar-pick = 从图像取色

# 调色板贴图。
pin-menu-copy-hex = 复制 HEX
pin-menu-copy-rgb = 复制 RGB
pin-menu-copy-hsl = 复制 HSL
pin-menu-pick = 取色
pin-menu-roles = MD3 色板
pin-menu-add-colour = 添加颜色
pin-menu-remove-colour = 删除颜色
pin-palette-code = 色值
pin-code-hex = HEX
pin-code-rgb = RGB
pin-code-hsl = HSL

# 贴图佩戴的徽标。
pin-badge-hdr = HDR
pin-badge-clipboard = 剪贴板

# 命令行。
cli = CrabSnap：截取屏幕，并留下其中一块
cli-lang = 本次运行的界面语言：auto 或语言标签
cli-no_daemon = 不使用 daemon：在本进程中执行命令
cli-shot = 抓取屏幕，写进文件或贴到屏上
cli-shot-file = 把截图写到这里；省略时落在截图目录
cli-shot-format = 写出的格式（png、jpg、webp、avif、jxl、tiff、bmp、gif、qoi）；省略时先看文件名，再看配置
cli-pin-export-format = 写出的格式（png、jpg、webp、avif、jxl、tiff、bmp、gif、qoi）；省略时看文件名
cli-shot-pin = 把截图贴到屏上，而不是写文件
cli-shot-delay = 抓取前等待的秒数
cli-shot-output = 抓一个输出，不弹覆盖层
cli-shot-output-clipboard = 把截图放进剪贴板；命令退出后仍由 daemon 提供
cli-shot-output-output = 抓这个输出；省略时抓第一个
cli-shot-output-backend = 固定抓取后端（ext、screencopy、portal）
cli-shot-region = 冻结屏幕并拖一个选区
cli-shot-region-demo = 画一个固定选区，不必真的拖动
cli-pin = 操作屏上的贴图
cli-pin-add = 把文件贴到屏上
cli-pin-add-file = 要贴的文件，每个文件一张贴图
cli-pin-add-clipboard = 贴剪贴板里的内容：文件列表每个文件一张贴图，一张图片贴一次
cli-pin-add-as = 按这个类型理解文件，而不是按内容判断：bitmap、vector、text、markdown、code、latex 或 palette
cli-pin-list = 列出 daemon 持有的贴图
cli-pin-export = 把屏上的一张贴图编码进文件
cli-pin-export-id = 哪一张，用 `pin list` 打印的 id
cli-pin-export-file = 把编码结果写到这里
cli-pin-edit = 打开一张贴图的标注编辑器
cli-pin-edit-id = 哪一张，用 `pin list` 打印的 id
cli-pin-close = 关闭一张贴图
cli-pin-close-id = 哪一张；省略时关掉指针下的那张
cli-pin-show = 重新显示所有贴图
cli-pin-hide = 隐藏所有贴图，但保留它们
cli-pin-toggle = 在显示与隐藏之间切换
cli-info = 关于构建与会话的只读回答
cli-info-config = 当前使用的配置
cli-info-formats = 这个构建能写出的编码
cli-info-formats-verbose = 连每个编码器自己的参数一起列出来
cli-info-status = 抓取后端、输出与 daemon 的状态
cli-daemon = 运行服务：一条连接、所有 surface、剪贴板
cli-daemon-idle = 空闲这么多秒后退出；省略时永不退出
cli-quit = 让运行中的 daemon 退出
cli-reload = 重新读取配置，并只重新探测被改动的那部分
cli-completions = 打印补全脚本
cli-completions-shell = 为哪个 shell 生成
