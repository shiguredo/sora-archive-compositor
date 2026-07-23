# `sora-archive-compositor list-codecs` コマンド

`sora-archive-compositor list-codecs` コマンドは、Sora Archive Compositor で利用可能なコーデックの一覧を表示するためのコマンドです。
このコマンドは、使用可能なエンコーダーやデコーダーの情報を JSON 形式で出力します。

## 使用方法

```console
$ sora-archive-compositor list-codecs -h
Sora Archive Compositor

Usage: sora-archive-compositor ... list-codecs [OPTIONS]

Options:
  -h, --help            このヘルプメッセージを表示します ('--help' なら詳細、'-h' なら簡易版を表示)
      --version         バージョン番号を表示します
      --verbose         警告未満のログメッセージも出力します
      --openh264 <PATH> OpenH264 の共有ライブラリのパス [env: SORA_ARCHIVE_COMPOSITOR_OPENH264_PATH]
      --fdk-aac <PATH>  FDK-AAC の共有ライブラリのパス [env: SORA_ARCHIVE_COMPOSITOR_FDK_AAC_PATH]
```

## 実行例

コマンドを実行すると、利用可能なコーデックの一覧が JSON 形式で出力されます。

出力内容は実行環境とオプションによって変わります。以下は macOS で `--openh264` と `--fdk-aac` を指定せずに実行した場合の例です。

```console
$ sora-archive-compositor list-codecs
{
  "codecs": [
    {
      "name": "OPUS",
      "type": "audio",
      "decoders": ["opus"],
      "encoders": ["opus"]
    },
    {
      "name": "AAC",
      "type": "audio",
      "decoders": [],
      "encoders": ["audio_toolbox"]
    },
    {
      "name": "VP8",
      "type": "video",
      "decoders": ["libvpx"],
      "encoders": ["libvpx"]
    },
    {
      "name": "VP9",
      "type": "video",
      "decoders": ["libvpx"],
      "encoders": ["libvpx"]
    },
    {
      "name": "H264",
      "type": "video",
      "decoders": ["video_toolbox"],
      "encoders": ["video_toolbox"]
    },
    {
      "name": "H265",
      "type": "video",
      "decoders": ["video_toolbox"],
      "encoders": ["video_toolbox"]
    },
    {
      "name": "AV1",
      "type": "video",
      "decoders": ["dav1d"],
      "encoders": ["svt_av1"]
    }
  ],
  "engines": [
    {
      "name": "audio_toolbox"
    },
    {
      "name": "dav1d",
      "repository": "https://github.com/videolan/dav1d.git",
      "build_version": "1.5.1"
    },
    {
      "name": "libvpx",
      "repository": "https://github.com/webmproject/libvpx.git",
      "build_version": "v1.15.2"
    },
    {
      "name": "opus",
      "repository": "https://github.com/xiph/opus.git",
      "build_version": "v1.5.2"
    },
    {
      "name": "svt_av1",
      "repository": "https://gitlab.com/AOMediaCodec/SVT-AV1.git",
      "build_version": "v3.1.0"
    },
    {
      "name": "video_toolbox"
    }
  ]
}
```

OpenH264 は `--openh264` を指定して共有ライブラリをロードできた場合にのみ表示されます。FDK-AAC も同様に `--fdk-aac` を指定した場合のみ表示されます。Linux では `audio_toolbox` / `video_toolbox` は表示されません。

`codecs` には、その環境の Sora Archive Compositor が利用可能なコーデック一覧と、
それぞれのコーデックのデコードおよびエンコードに使用されるエンジン名が表示されます。
あるコーデックのデコード・エンコードに対応するエンジンが複数ある場合には、リストの先頭要素のものが使用されます。

`engines` には、各エンジンの詳細情報が載っています。
