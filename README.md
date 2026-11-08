<div align="center">

<h1>LEGENDA</h1>

<p><strong>Baixe legendas do OpenSubtitles e de fontes públicas em uma interface nativa acelerada por GPU escrita em Rust com GPUI.</strong></p>

<p>
  <a href="https://github.com/desenrolado-s/Subtitle-Downloader/actions/workflows/build.yml"><img src="https://github.com/desenrolado-s/Subtitle-Downloader/actions/workflows/build.yml/badge.svg" alt="Build and test"></a>
  <a href="https://github.com/desenrolado-s/Subtitle-Downloader/releases/latest"><img src="https://img.shields.io/github/v/release/desenrolado-s/Subtitle-Downloader?label=release" alt="Última release"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-2024-DEA584?logo=rust&logoColor=white" alt="Rust 2024"></a>
  <a href="https://gpui.rs/"><img src="https://img.shields.io/badge/GPUI-0.2-084CCF" alt="GPUI"></a>
  <a href="#requisitos"><img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-3776AB" alt="Linux, macOS e Windows"></a>
</p>

</div>

## Visão geral

LEGENDA é um aplicativo desktop para Linux, macOS e Windows escrito em **Rust** que pesquisa e baixa legendas do OpenSubtitles e de fontes públicas. A interface utiliza **GPUI** (o framework de interface acelerado por GPU do Zed) e executa todas as operações de rede em segundo plano, mantendo a janela responsiva.

O vídeo nunca é enviado ao serviço. A busca usa o hash de 64 bits do arquivo quando ele está vinculado e, se necessário, recorre ao título.

## Recursos

- Busca por título ou por hash do vídeo.
- Vínculo opcional de um arquivo de vídeo local.
- Filtro por idioma, como `pt-br`, `pt`, `en`, `es` e `ja`.
- Download em SRT com validação de conteúdo e gravação atômica sem sobrescrita.
- Credenciais guardadas no cofre seguro do sistema (Secret Service via `oo7` no Linux, Keychain no macOS e Credential Manager no Windows).
- Interface escura, minimalista e acelerada por GPU com **GPUI**.

## Requisitos

- Linux com Wayland ou X11 (Vulkan), macOS ou Windows 10/11 64 bits.
- Rust 1.88+ (`cargo` e `rustc`).
- Cofre de credenciais do sistema (no Linux, Secret Service como GNOME Keyring ou KWallet).
- Conta e chave de API do OpenSubtitles, apenas para o catálogo OpenSubtitles.

## Compilação e execução

Clone o repositório e execute o aplicativo com Cargo:

```sh
git clone https://github.com/desenrolado-s/Subtitle-Downloader.git
cd Subtitle-Downloader
cargo run --release
```

Para compilar no Linux, instale as bibliotecas de desenvolvimento do sistema conforme a sua distribuição:

| Distribuição | Comando |
| --- | --- |
| Arch Linux | `sudo pacman -S base-devel pkgconf libxcb libxkbcommon wayland fontconfig freetype2 vulkan-icd-loader` |
| Ubuntu / Debian | `sudo apt install build-essential pkg-config libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libfontconfig1-dev libfreetype6-dev libssl-dev` |
| Fedora | `sudo dnf install gcc pkgconf-pkg-config libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel wayland-devel fontconfig-devel freetype-devel openssl-devel` |

## Atualizações do AppImage

Cada release publica um arquivo `.zsync` estável em:

```text
https://github.com/desenrolado-s/Subtitle-Downloader/releases/latest/download/Legenda-linux-x64.AppImage.zsync
```

O AppImage tem essa informação de atualização embutida, permitindo baixar apenas os blocos alterados com o `AppImageUpdate` ou `appimageupdatetool`.

## Configuração da conta

1. Crie uma conta em [opensubtitles.com](https://www.opensubtitles.com).
2. Gere uma chave de aplicação em [opensubtitles.com/consumers](https://www.opensubtitles.com/consumers).
3. Abra **Conta** na barra superior e informe chave, usuário e senha.
4. Se quiser manter os dados entre sessões, use **Salvar no cofre**.

A chave e o usuário também podem ser definidos pelas variáveis de ambiente `OPENSUBTITLES_API_KEY` e `OPENSUBTITLES_USERNAME`. A senha é solicitada apenas na interface e nunca é gravada em texto puro.

## Fontes sem chave de API

- **Gestdown**: API pública que fornece legendas do catálogo Addic7ed por HTTPS para episódios de séries (`Minha.Serie.S01E02.mkv` ou `Minha Série S01E02`), sem chave ou conta.
- **Subtitulamos.tv**: Fonte pública sem conta para episódios de TV em inglês (`en`) e português (`pt`, `pt-br`).
- **Podnapisi**: Busca pública multilíngue para filmes e séries (`en`, `pt`, `ja`, `es`, `fr`, `de`, `it`, `ko`, `ru`, `zh`), com extração segura de arquivos `.srt`.
- **AJATT / Kitsunekko**: Catálogo público de legendas japonesas (`.srt`) para animes com o idioma `ja`.

## Como usar

1. Digite o nome do filme ou da série. Vincular um vídeo é opcional.
2. Ajuste o idioma se necessário.
3. Clique em **Buscar legendas →** (ou pressione `Enter`) e selecione uma versão na lista.
4. Clique em **Baixar selecionada ↓**.

Com um vídeo vinculado, a legenda é salva ao lado dele no formato `filme.pt-br.srt`, reconhecido por players como VLC e mpv. Sem vídeo, o aplicativo abre o diálogo nativo **Salvar como**.

## Estrutura do projeto

| Caminho | Responsabilidade |
| --- | --- |
| `src/main.rs` | Ponto de entrada do executável `Legenda` e modo `--smoke-test`. |
| `src/lib.rs` | Exportação dos módulos da biblioteca `legenda`. |
| `src/ui.rs` | Interface desktop em GPUI, componentes de entrada, modal de conta e tarefas assíncronas. |
| `src/service.rs` | Coordenação de busca e download independente da interface. |
| `src/provider.rs` | Cliente REST do OpenSubtitles e tratamento de erros HTTP. |
| `src/gestdown.rs` | Fonte pública Gestdown/Addic7ed para episódios de TV. |
| `src/subtitulamos.rs` | Fonte pública Subtitulamos.tv para episódios em inglês e português. |
| `src/podnapisi.rs` | Fonte pública multilíngue Podnapisi para filmes e séries. |
| `src/ajatt.rs` | Catálogo público AJATT/Kitsunekko de legendas japonesas para anime. |
| `src/files.rs` | Cálculo do hash de 64 bits do vídeo, validação de SRT e gravação atômica. |
| `src/credentials.rs` | Serialização e armazenamento das credenciais no cofre seguro do sistema. |
| `tests/integration.rs` | Testes de integração de versionamento, busca multi-provedor e armazenamento. |
| `packaging/linux/` | Arquivo `.desktop` e ícones do aplicativo. |
| `scripts/build-appimage.sh` | Empacotamento do binário Rust em AppImage. |
| `.github/workflows/build.yml` | Testes, builds nativos para Linux/macOS/Windows e publicação de releases. |

## Testes

Execute a suíte completa de testes unitários e de integração:

```sh
cargo test
```

Para executar o teste rápido de abertura e renderização da janela GPUI:

```sh
cargo run -- --smoke-test
```
