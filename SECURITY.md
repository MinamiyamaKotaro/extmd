# セキュリティポリシー

## サポート対象バージョン

`extmd`はセマンティックバージョニングに従います。脆弱性修正は
[crates.io](https://crates.io/crates/extmd)で公開中の最新バージョンに対してのみ行い、
過去バージョンへのバックポートは行いません。

| バージョン | サポート状況 |
| --- | --- |
| 最新版 | ✅ |
| それ以前 | ❌ |

## 脆弱性の報告方法

脆弱性を発見した場合は、公開のIssueやPull Requestではなく、GitHubの
[Private vulnerability reporting](https://github.com/MinamiyamaKotaro/extmd/security/advisories/new)
から報告してください。第三者に影響が及ぶ前に対応するため、非公開の報告経路を使用してください。

報告には可能な範囲で以下の情報を含めてください。

- 影響を受けるバージョン
- 再現手順（可能であればサンプルの`.xlsx`ファイルや入力）
- 想定される影響範囲

## 依存関係の脆弱性チェック

依存クレートの既知脆弱性は[RustSec Advisory Database](https://rustsec.org/)を用いた
`cargo audit`により、`Cargo.toml`/`Cargo.lock`を変更するPRで自動チェックしています
（[`.github/workflows/security-audit.yml`](.github/workflows/security-audit.yml)）。
