# Windows WSL2でMISAKAローカルnodeを起動する

このフォルダは、WindowsからPowerShellでWSL2 Ubuntuを呼び出し、MISAKA nodeをローカルPC上で動かすための入口です。

Windows nativeで直接動かす方式ではありません。安定性と実装のシンプルさを優先して、Windows側の操作はPowerShell、実行本体はWSL2 Ubuntuに寄せています。

## まず必要なもの

- Windows 10/11
- WSL2 Ubuntu
- PowerShell
- このzipを展開したフォルダ

WSL2 Ubuntuがまだない場合は、`windows/install-ubuntu-wsl.cmd` を実行します。

またはPowerShellで:

```powershell
wsl --install -d Ubuntu
```

インストール後、Windowsの再起動を求められた場合は再起動してください。
その後、Ubuntuを一度開いて初期ユーザー作成まで終わらせます。

## 一番簡単な起動

zipを展開したあと、Explorerで以下をダブルクリックします。

```text
windows/start-node-wsl.cmd
```

重要: `windows` フォルダだけをコピーしないでください。
同じ階層に `scripts`, `ui`, `mac`, `docs` も必要です。

正しい配置例:

```text
C:\misakatest\windows\start-node-wsl.cmd
C:\misakatest\scripts\misaka-desktop-node.sh
C:\misakatest\ui\setup.html
```

これでPowerShellが開き、WSL2 Ubuntu内で以下を自動実行します。

```text
依存確認
Rust準備
MISAKA source clone
release build
kaspad起動
node状態確認
```

初回buildはかなり時間がかかります。途中でPowerShellを閉じないでください。

## よく使う入口

| ファイル | 内容 |
| --- | --- |
| `windows/start-node-wsl.cmd` | node準備から起動まで |
| `windows/start-web-ui-wsl.cmd` | ローカルWeb UIを起動 |
| `windows/check-status-wsl.cmd` | 現在の状態確認 |
| `windows/prepare-validator-wsl.cmd` | validator準備を進める |
| `windows/stop-all-wsl.cmd` | node / miner / validatorを停止 |
| `windows/list-distros-wsl.cmd` | WSL distro一覧 |
| `windows/install-ubuntu-wsl.cmd` | UbuntuをWSLへインストール |

## PowerShellから直接使う

zip展開先のフォルダでPowerShellを開いて実行します。

```powershell
powershell -ExecutionPolicy Bypass -File .\windows\start-misaka-local-node-wsl.ps1 -Command auto-node
```

状態確認:

```powershell
powershell -ExecutionPolicy Bypass -File .\windows\start-misaka-local-node-wsl.ps1 -Command status
```

エラー報告用support log作成:

```powershell
powershell -ExecutionPolicy Bypass -File .\windows\start-misaka-local-node-wsl.ps1 -Command collect-support-log
```

validator準備:

```powershell
powershell -ExecutionPolicy Bypass -File .\windows\start-misaka-local-node-wsl.ps1 -Command auto-validator
```

Bond作成:

```powershell
powershell -ExecutionPolicy Bypass -File .\windows\start-misaka-local-node-wsl.ps1 -Command bond -Amount 10MSK
```

全停止:

```powershell
powershell -ExecutionPolicy Bypass -File .\windows\start-misaka-local-node-wsl.ps1 -Command stop-all
```

ローカルWeb UI:

```powershell
powershell -ExecutionPolicy Bypass -File .\windows\start-misaka-local-node-wsl.ps1 -Command web
```

Web UIはWSL2 Ubuntu側で動き、Windowsのブラウザから `http://127.0.0.1:<port>` を開きます。
画面構成はVPS版のsetup/dashboardに寄せています。
初回はWSL Ubuntuの依存packageを入れるため、PowerShell windowでUbuntuのsudo passwordを聞かれる場合があります。これはWindowsのパスワードではなく、Ubuntuを初回起動した時に作ったユーザーのパスワードです。
ブラウザタブを閉じても、`windows/start-web-ui-wsl.cmd` をもう一度実行すれば、起動中のWeb UIを検出して同じURLを開き直します。
PowerShell windowを閉じるとWeb UIも止まりますが、起動済みnode自体は `stop-all` するまで動き続けます。

## 複数のWSL distroがある場合

一覧:

```powershell
powershell -ExecutionPolicy Bypass -File .\windows\start-misaka-local-node-wsl.ps1 -ListDistros
```

Ubuntuを明示:

```powershell
powershell -ExecutionPolicy Bypass -File .\windows\start-misaka-local-node-wsl.ps1 -Distro Ubuntu -Command auto-node
```

`Ubuntu-24.04` のような名前の場合は、その名前を指定します。

## `null 値の式ではメソッドを呼び出せません` が出る場合

古いzipでは、WSLのdistro一覧取得やWindowsパスからWSLパスへの変換に失敗した時、PowerShell内部のエラーとしてこの表示になる場合がありました。

最新zipでは、空の値を安全に扱い、原因が分かるエラーを出すようにしています。もしこのエラーが出る場合は、まず最新zipを展開し直してください。

それでも失敗する場合は、次を確認してください。

- Ubuntuを一度起動し、初期ユーザー作成まで終わっている
- zipを `C:\Users\<you>\Downloads` など通常のローカルフォルダに展開している
- `C:\misakatest` のような短い通常フォルダでも利用できます
- OneDrive同期フォルダ、ネットワークドライブ、特殊な権限のフォルダから起動していない
- `windows/list-distros-wsl.cmd` でUbuntuが表示される

もし `wslpath: C:misakatest` のように `\` が消えたエラーが出る場合は、古いzipです。最新zipではWindowsパスを `C:/misakatest` 形式へ変換してからWSLへ渡します。

## `chmod: cannot access 'scripts/misaka-desktop-node.sh'` が出る場合

これは、起動した `windows` フォルダの隣に `scripts` フォルダがない状態です。

よくある原因:

- zipを全部展開せず、`windows` フォルダだけをコピーしている
- zipを展開した後、内側のフォルダ構成を崩している
- 古いzipを使っている

最新zipでは起動前にこの配置を確認し、分かりやすいエラーを出します。`windows`, `scripts`, `ui`, `mac`, `docs` を同じフォルダ内に置いてください。

## Web UIのPrepareが止まって見える場合

古い版では、Web UIから `Prepare` を押した時にWSLの `sudo apt install` がパスワード待ちになり、ブラウザから入力できず止まったように見える場合がありました。

最新zipでは、Web UI起動時に必要ならPowerShell window側で先にsudo確認を行います。もしブラウザ側にsudo関連のエラーが出る場合は、`windows/start-web-ui-wsl.cmd` を起動し直し、PowerShell windowに表示されるUbuntu password入力を完了してから続けてください。

## データの保存先

nodeのデータやbuild済みbinaryは、WSL2 Ubuntu側の以下に保存されます。

```text
~/.misaka-desktop-node/
```

つまり、Windowsのzip展開フォルダを消しても、WSL内のnodeデータは残ります。

完全に消したい場合:

```powershell
powershell -ExecutionPolicy Bypass -File .\windows\start-misaka-local-node-wsl.ps1 -Command clean
```

## 注意

- PCをスリープするとnode/validator/minerは止まります
- 自宅回線では外部peerから入りにくい場合があります
- 長期安定運用はVPS版を推奨します
- Windows native実行ではなく、WSL2 Ubuntuを利用します
