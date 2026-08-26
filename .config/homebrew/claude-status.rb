# GENERATED — the source of this file lives in virajp/claude-status at
# `.config/homebrew/claude-status.rb`. Edit it there, not in the tap.
#
# `release.yml`'s `bump-tap` job rewrites `url` and `sha256` for the release it
# has just published and writes this whole file into virajp/homebrew-tap. The
# tap's copy is overwritten in full every release, so it cannot drift, and the
# first release creates it — there is no formula to seed by hand.
#
# The whole formula lives in the source repository rather than a two-field patch
# against a copy in the tap because `desc`, `homepage`, `caveats` and the
# `depends_on` pair would otherwise exist only in the tap, where nothing in the
# project's test suite can see them.
#
# `url` and `sha256` are a real released pair rather than placeholders, so this
# file is a formula `brew style` and `brew audit` can check wherever it sits.
class ClaudeStatus < Formula
  desc "Status line for Claude Code"
  homepage "https://claude-status.virajp.dev"
  url "https://github.com/virajp/claude-status/releases/download/v0.1.0/claude-status-darwin-arm64.tar.gz"
  sha256 "af64e2a6ed8c0b27d6d9a0473ab5b8c9b7ce1cf123d720f4612c68ae58a9a044"
  license "MIT"

  # No `version`. Homebrew scans it out of the url, and a `version` line beside
  # a version-bearing url is a hard `brew audit` failure.

  # `ArchRequirement` is `fatal true`, so an Intel Mac is refused with "The
  # arm64 architecture is required for this software." rather than installing
  # something that cannot run. `depends_on :macos` handles the Linux half.
  # `arch` before `macos` — `FormulaAudit/DependencyOrder` enforces the order.
  depends_on arch: :arm64
  depends_on :macos

  def install
    # The archive carries the binary at its root, which is the only thing
    # `bin.install` reads. `reproducible_tar`'s `-C` is what puts it there.
    bin.install "claude-status"
  end

  def caveats
    # Shown after `brew install` AND by `brew info --formula`, from this one
    # block. The overwrite warning belongs here because this is the last text a
    # user reads before running the command that does it.
    <<~EOS
      Run `claude-status --configure` to wire this into Claude Code.
      This OVERWRITES any existing status line in ~/.claude/settings.json.

      Docs and the config generator:
        https://claude-status.virajp.dev
    EOS
  end

  test do
    # §5 guarantees `--version` prints the bare version and nothing else, which
    # makes it the one output shape safe to match on.
    assert_match version.to_s, shell_output("#{bin}/claude-status --version")
  end
end
