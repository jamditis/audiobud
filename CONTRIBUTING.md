# Contributing to AudioBud

Thank you for your interest in contributing to AudioBud! This guide will help you get started with contributing to this open source speech-to-text application.

## Priorities

AudioBud supports Windows x64 and Apple Silicon macOS. Bug fixes and stability
improvements are the most welcome contributions. New features are tracked as
issues. Open one, or comment on an existing issue, before starting a large
change so we can agree on scope first.

## 📖 Philosophy

AudioBud builds on [Handy](https://github.com/cjpais/Handy)'s goal of being a simple, forkable speech-to-text app — a well-patterned codebase that is easy to build on. We prioritize:

- **Simplicity**: Clear, maintainable code over clever solutions
- **Extensibility**: Make it easy for others to fork and customize
- **Privacy**: Keep everything local and offline
- **Accessibility**: Free tooling that belongs in everyone's hands

## 🚀 Getting started

### Prerequisites

Before you begin, ensure you have the following installed:

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager
- Platform-specific build tools (see [BUILD.md](BUILD.md))

### Set up your development environment

1. **Fork the repository** on GitHub

2. **Clone your fork**:

   ```bash
   git clone git@github.com:YOUR_USERNAME/audiobud.git
   cd audiobud
   ```

3. **Add upstream remote**:

   ```bash
   git remote add upstream git@github.com:cjpais/Handy.git
   ```

4. **Install dependencies**:

   ```bash
   bun install --frozen-lockfile
   ```

5. **Download required models**:

   ```bash
   mkdir -p src-tauri/resources/models
   curl -L -o src-tauri/resources/models/silero_vad_v4.onnx https://github.com/jamditis/audiobud/releases/download/model-assets-v1/silero_vad_v4.onnx
   ```

6. **Run in development mode**:
   ```bash
   bun run tauri dev
   # On macOS if you encounter cmake errors:
   CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
   ```

For detailed platform-specific setup instructions, see [BUILD.md](BUILD.md).

### Understand the codebase

AudioBud follows a clean architecture pattern:

**Backend (Rust - `src-tauri/src/`):**

- `lib.rs` - Main application entry point with Tauri setup
- `managers/` - Core business logic (audio, model, transcription)
- `audio_toolkit/` - Low-level audio processing (recording, VAD)
- `commands/` - Tauri command handlers for frontend communication
- `shortcut/` - Global keyboard shortcut handling
- `settings.rs` - Application settings management

**Frontend (React/TypeScript - `src/`):**

- `App.tsx` - Main application component
- `components/` - React UI components
- `hooks/` - Reusable React hooks
- `lib/types.ts` - Shared TypeScript types

For more details, see the project layout in [README.md](README.md) or
[AGENTS.md](AGENTS.md).

## 🐛 Report bugs

### Before you submit a bug report

1. **Search existing issues** at [github.com/jamditis/audiobud/issues](https://github.com/jamditis/audiobud/issues), including closed ones
2. **Try the latest release** to see if the issue has been fixed
3. **Enable debug mode** (`Cmd/Ctrl+Shift+D`) to gather diagnostic information

### Submit a bug report

When creating a bug report, please include:

**System Information:**

- App version (found in settings or about section)
- Operating System (e.g., macOS 14.1, Windows 11, Ubuntu 22.04)
- CPU (e.g., Apple M2, Intel i7-12700K, AMD Ryzen 7 5800X)
- GPU (e.g., Apple M2 GPU, NVIDIA RTX 4080, Intel UHD Graphics)

**Bug Details:**

- Clear description of the bug
- Steps to reproduce
- Expected behavior
- Actual behavior
- Screenshots or logs if applicable
- Information from debug mode if relevant

Use the [Bug Report template](.github/ISSUE_TEMPLATE/bug_report.md) when creating an issue.

## 💡 Suggest features

AudioBud has GitHub Discussions disabled, so feature requests are filed as issues with the `enhancement` label. This keeps everything in one tracker.

### Before you suggest a feature

1. **Search existing issues** at [github.com/jamditis/audiobud/issues](https://github.com/jamditis/audiobud/issues), including closed ones, to avoid duplicates

### Submit a feature request

1. Open a [new issue](https://github.com/jamditis/audiobud/issues/new) and add the `enhancement` label
2. Describe your feature idea including:
   - The problem you're trying to solve
   - Your proposed solution
   - Any alternatives you've considered
   - How it fits with AudioBud's goals

## 🔧 Make code contributions

### Before you start

**This is critical:** Before writing any code, please do the following:

1. **Search existing issues and PRs** - Check both open AND closed issues and pull requests. Someone may have already addressed this, or there may be a reason it was closed.
   - [Open issues](https://github.com/jamditis/audiobud/issues)
   - [Closed issues](https://github.com/jamditis/audiobud/issues?q=is%3Aissue+is%3Aclosed)
   - [Open PRs](https://github.com/jamditis/audiobud/pulls)
   - [Closed PRs](https://github.com/jamditis/audiobud/pulls?q=is%3Apr+is%3Aclosed)

2. **If something was previously closed** - If you want to revisit a closed issue or PR, provide a strong argument for why it should be reconsidered and link to the prior issue or PR.

3. **Agree on scope for larger changes** - For anything beyond a small fix, open or comment on an issue first so we can agree on the approach before you invest time. This keeps AudioBud focused and avoids feature creep.

### Development workflow

1. **Create a feature branch**:

   ```bash
   git checkout -b feature/your-feature-name
   # or
   git checkout -b fix/your-bug-fix
   ```

2. **Make your changes**:
   - Write clean, maintainable code
   - Follow existing code style and patterns
   - Add comments for complex logic
   - Keep commits focused and atomic

3. **Test thoroughly**:
   - Test on your target platform(s)
   - Verify existing functionality still works
   - Test edge cases and error conditions
   - Use debug mode to verify audio/transcription behavior

4. **Commit your changes**:

   ```bash
   git add .
   git commit -m "feat: add your feature description"
   # or
   git commit -m "fix: describe the bug fix"
   ```

   Use conventional commit messages:
   - `feat:` for new features
   - `fix:` for bug fixes
   - `docs:` for documentation changes
   - `refactor:` for code refactoring
   - `test:` for test additions/changes
   - `chore:` for maintenance tasks

5. **Keep your fork updated**:

   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

6. **Push to your fork**:

   ```bash
   git push origin feature/your-feature-name
   ```

7. **Create a pull request**:
   - Go to the [AudioBud repository](https://github.com/jamditis/audiobud)
   - Click "New Pull Request"
   - Select your fork and branch
   - Fill out the PR template completely, including:
     - Clear description of changes
     - Links to related issues
     - How you tested the changes
     - Screenshots/videos if applicable
     - Breaking changes (if any)

### AI assistance disclosure

**AI-assisted PRs are welcome!** Use whatever tools help you contribute, just be upfront about it.

In your PR description, please include:

- Whether AI was used (yes/no)
- Which tools were used (e.g., "Claude Code", "GitHub Copilot", "ChatGPT")
- How extensively it was used (e.g., "generated boilerplate", "helped debug", "wrote most of the code")

### Code style guidelines

**Rust:**

- Follow standard Rust formatting (`cargo fmt`)
- Run `cargo clippy` and address warnings
- Use descriptive variable and function names
- Add doc comments for public APIs
- Handle errors explicitly (avoid unwrap in production code)

**TypeScript/React:**

- Use TypeScript strictly, avoid `any` types
- Follow React hooks best practices
- Use functional components
- Keep components small and focused
- Use Tailwind CSS for styling

**General:**

- Write self-documenting code
- Add comments for non-obvious logic
- Keep functions small and single-purpose
- Prioritize readability over cleverness

### Test your changes

Run the automated checks before manual testing:

```bash
bun run test
bun run lint
bun run format:check
bun run check:translations
bun run check:rebrand
bun run build
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
```

**Manual testing:**

- Run the app in development mode: `bun run tauri dev`
- Test your changes with debug mode enabled
- Verify on multiple platforms if possible
- Test with different audio devices
- Try various transcription scenarios
- On macOS, revoke and restore Microphone and Accessibility permissions, then
  confirm that AudioBud detects each state change.
- On macOS, test shortcut changes and slider pointer, keyboard, and blur commits
  in the packaged WebKit app.

**Build for production:**

```bash
bun run tauri build
```

Test the production build to ensure it works as expected.

## 📝 Documentation contributions

Documentation improvements are highly valued! You can contribute by:

- Improving README.md, BUILD.md, or this CONTRIBUTING.md
- Adding code comments and doc comments
- Creating tutorials or guides
- Improving error messages
- Updating the project website content

## 🤝 Community guidelines

- **Be respectful and inclusive** - We welcome contributors of all skill levels
- **Be patient** - This is maintained by a small team, responses may take time
- **Be constructive** - Focus on solutions and improvements
- **Be collaborative** - Help others and share knowledge
- **Search first** - Check existing issues/discussions before creating new ones

## 🎯 Good first issues

Look for issues labeled `good first issue` or `help wanted` if you're new to the project. These are typically:

- Well-defined and scoped
- Good for learning the codebase
- Mentor support available

## 📞 Get help

- **Issues**: Open an issue at [github.com/jamditis/audiobud/issues](https://github.com/jamditis/audiobud/issues) for bugs, questions, or feature requests

## 📜 License

By contributing to AudioBud, you agree that your contributions will be licensed under the MIT License. See [LICENSE](LICENSE) for details.

---

**Thank you for contributing to AudioBud!** Your efforts help make speech-to-text technology more accessible, private, and extensible for everyone.
