# 2025.3.2 の Dockerfile と canary.py を移植する

- Priority: Low
- Created: 2026-07-23
- Completed: {YYYY-MM-DD}
- Model: Opus 4.7
- Branch: feature/add-dockerfile-and-canary
- Polished: {YYYY-MM-DD}

## 目的

hisui 2025.3.2 に含まれていた `Dockerfile` (Docker 環境でのビルド定義) と `canary.py` (canary 検証スクリプト) を sora-archive-compositor に移植する。

## 優先度根拠

日常の動作には不要だが、Docker ビルドや canary 検証が Sora 側との組み合わせ検証で有用。Phase 1 の中では優先度が最も低いので Low とする。

## 現状

sora-archive-compositor に `Dockerfile` と `canary.py` は存在しない。

## 設計方針

- 2025.3.2 の `Dockerfile` / `canary.py` を移植する
- ツール名 / パッケージ名 / リポジトリ名の参照を sora-archive-compositor に置換する
- Docker のベースイメージ・ビルド手順・実行手順は 2025.3.2 と同じ意味で維持する
- canary.py の運用ロジックはそのまま維持し、参照するバイナリ名・パッケージ名のみ置換する

## 移植対象

- `Dockerfile`
- `canary.py`

## 完了条件

- `Dockerfile` と `canary.py` が sora-archive-compositor のルートに配置される
- `Dockerfile` 内の `hisui` 参照が sora-archive-compositor 向けに置換されている (バイナリ名、パッケージ名、イメージタグ名等)
- `canary.py` 内のバイナリ名・パッケージ名参照が置換されている
- `docker build .` が成功する (コア移植完了後の状態で)
- `canary.py` を手動実行して sora-archive-compositor バイナリと動作することを確認する
- `prek.toml` の builtin hooks に `{ id = "check-executables-have-shebangs" }` を再追加する (testdata 移植時に暫定撤去してある。canary.py が executable として commit されると対象が生まれ、`check-hooks-apply` を通過できる)
- `docs/docker.md` の版数タグを sora-archive-compositor の版数体系に追随させる。対象は L14 の `docker pull ghcr.io/shiguredo/sora-archive-compositor:2025.1.0`、L17 の同 `:2025.1.0-canary.8`、L117 のタグ戦略の例示 `例: 2025.1.0` の 3 箇所。いずれも hisui 時代の版数で、sora-archive-compositor には来ない (`Cargo.toml` は `2026.1.0-canary.0`)。同ファイル L34 の `--version` 応答例は `sora-archive-compositor 2026.1.0-canary.0` なのでファイル内部でも矛盾している。docs 移植時にこの追随を「Dockerfile 移植完了後、または各リリース時の Phase 2 対応で扱う」と委任した分

## 解決方法

1. `git show 2025.3.2:Dockerfile` / `git show 2025.3.2:canary.py` で内容を取得する
2. `hisui` 参照を確認し、コマンド名・パッケージ名・ダウンロード URL 等を sora-archive-compositor に置換する
3. `canary.py` に executable bit を付与する (`chmod +x canary.py`)。shebang (`#!/usr/bin/env python3` 等) が hisui 2025.3.2 のものを維持していることを確認する
4. `prek.toml` の builtin hooks に `{ id = "check-executables-have-shebangs" }` を再追加する
5. Docker ビルドを実行して確認する
6. canary.py の実行を確認する
7. `prek run --all-files` で `check-hooks-apply` を含む全フックが pass することを確認する

## 依存

- コア移植 (Cargo 依存と `src/` の取り込み) の完了が前提 (バイナリが存在しないと動作確認できないため)

## 参考

- 移植元: `../hisui@2025.3.2:Dockerfile`, `../hisui@2025.3.2:canary.py`

## pending にした理由 (2026-07-27)

OSS 公開前に canary リリースを publish したくないため pending にする (ユーザー判断)。

### 主因: canary.py の移植は publish の導線を用意することと同義

hisui 2025.3.2 の `canary.py` は以下を一連で実行する:

1. `Cargo.toml` の `[package] version` を `-canary.N` の連番で bump する
2. `cargo update <package>` を実行する
3. `[canary] Bump version to <version>` でコミットする
4. `git tag <version>` → `git push` → `git push origin <version>` を実行する

タグ push は hisui 2025.3.2 の `.github/workflows/release.yml` の起動条件であり、GitHub Release の作成と `ghcr.io/<repository>` への `docker buildx build --push` が走る。つまり完了条件の「`canary.py` を手動実行して sora-archive-compositor バイナリと動作することを確認する」を満たすには canary リリースを実際に publish する必要があり、公開前の方針では実行できない。

### 副因: 完了条件の `docker build .` が単体では成立しない

2025.3.2 の Dockerfile は `COPY hisui.${TARGETARCH} /usr/local/bin/hisui` の形で、ビルドコンテキストにクロスビルド済みバイナリ (`hisui.amd64` / `hisui.arm64`) が置かれていることを前提にしている。これを配置するのは `release.yml` の job (CI アーティファクトを download して `mv` する) だが、sora-archive-compositor に `release.yml` は存在しない。現行の CI 整備は `ci.yml` のみを対象にしており、`release.yml` は Phase 3 のスコープ整理で「一旦不要」と判断済み。したがって Dockerfile 単体では完了条件の `docker build .` を満たす手段が現状ない。

### reopened にする条件

以下のいずれかが満たされたとき reopened にする:

- OSS 公開後に canary リリースを実際に出す方針が確定する
- `release.yml` (クロスビルド成果物の生成) の移植方針が決まり、Dockerfile 単体の検証手段が確定する
- Dockerfile を「ビルド済みバイナリを COPY する」形から「コンテナ内でビルドする」形に作り替える方針が決まる (この場合は設計方針の「Docker のベースイメージ・ビルド手順・実行手順は 2025.3.2 と同じ意味で維持する」の見直しを伴う)

### pending 中に残る事項

- `docs/docker.md` の `ghcr.io/shiguredo/sora-archive-compositor:*` は実在しない image を指したままになる。公開時点で image が存在していれば案内としては成立する。ただし版数タグ 3 箇所は公開を待っても存在しない版数なので、完了条件に追随項目として明記した
- `prek.toml` の builtin hooks への `check-executables-have-shebangs` 再追加も保留される (本 issue の完了条件に含まれているため)

## reopened にした理由 (2026-08-03)

hisui からのコードの移植や更新はほぼ終わって、そろそろ Docker 対応も進められる状態になったため。実際の作業は公開後になると思うが、その際に対応を忘れないように pending から戻しておく。公開前の現時点では、pending にした理由に書いた publish 要件 (canary.py のタグ push → release.yml 経由の Docker ビルド) は変わっていない。
