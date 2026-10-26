# 项目图标数据与许可

项目图标由 Unicode 表情和 Lucide 线性图标组成。表情名称、标签和分类来自 Emojibase 数据；Lucide 图标经 Iconify JSON 数据包分发。两类数据都随构建产物保存在本地，选择器打开时才加载。

| 数据或组件 | 版本 | 许可 | 许可文件 | 上游来源 |
| --- | --- | --- | --- | --- |
| Emojibase 数据 | 18.0.0 | MIT | [Emojibase-MIT.txt](Emojibase-MIT.txt) | [milesj/emojibase](https://github.com/milesj/emojibase) |
| Unicode Emoji 与 CLDR 注释 | 随 Emojibase 18.0.0 | Unicode License V3 | [Unicode-Data.txt](Unicode-Data.txt) | [Unicode Terms of Use](https://www.unicode.org/license.txt) |
| Lucide 图标 | 通过 @iconify-json/lucide 1.2.147 | ISC | [Lucide-ISC.txt](Lucide-ISC.txt) | [lucide-icons/lucide](https://github.com/lucide-icons/lucide) |
| @iconify-json/lucide | 1.2.147 | ISC | [Lucide-ISC.txt](Lucide-ISC.txt) | [Iconify Lucide 数据页](https://icon-sets.iconify.design/lucide/) |
| Feather Icons（部分 Lucide 图标源头） | 上游历史版本 | MIT | [Feather-MIT.txt](Feather-MIT.txt) | [feathericons/feather](https://github.com/feathericons/feather) |

表情使用系统彩色字体渲染，不随应用分发 emoji 图片。Lucide 图标以内联 SVG 路径写入脚本，颜色继承当前主题；Lucide 的部分图标源自 Feather Icons，许可文本一并保留。
