# Home Manager module for goto — URL shortener.
#
# Usage from a consuming flake:
#
#   inputs.goto.url = "github:tsauvajon/goto";
#   ...
#   imports = [ inputs.goto.homeManagerModules.default ];
#   programs.gotoLinks = {
#     enable = true;
#     apiUrl = "http://127.0.0.1:50002";
#     bookmarksFile = "/path/to/private/database.yml";
#   };
#
# The module is namespaced as `programs.gotoLinks` (not `programs.goto`)
# because home-manager already ships a `programs.goto` module for
# iridakos/goto, an unrelated directory-bookmarking shell tool.
#
# The module installs the goto binary (CLI + API) and writes
# `~/.config/goto/config.yml` from the supplied options. An optional
# private bookmarks file can be supplied to populate
# `~/.config/goto/database.yml`.
#
# This file is curried with the goto flake `self` so the module can
# locate the package without consumers re-resolving inputs.
self:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.programs.gotoLinks;
  inherit (lib)
    mkEnableOption
    mkOption
    mkIf
    types
    ;
in
{
  options.programs.gotoLinks = {
    enable = mkEnableOption "goto, a URL shortener with API/CLI/frontend";

    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "goto.packages.<system>.default";
      description = "The goto package to install.";
    };

    apiUrl = mkOption {
      type = types.str;
      example = "http://127.0.0.1:50002";
      description = ''
        URL the goto CLI talks to. Written into
        `~/.config/goto/config.yml`.
      '';
    };

    forceReplace = mkOption {
      type = types.bool;
      default = false;
      description = "Whether to overwrite existing bookmarks on collision.";
    };

    silent = mkOption {
      type = types.bool;
      default = false;
      description = "Suppress non-error output from the CLI.";
    };

    noBrowser = mkOption {
      type = types.bool;
      default = false;
      description = "Do not auto-open URLs in a browser.";
    };

    bookmarksFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        Optional path to a `database.yml` containing the user's
        bookmarks. When set, the file is symlinked into
        `~/.config/goto/database.yml`. Leave null to manage the
        bookmarks database directly through the CLI.
      '';
    };

    apiKey = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = ''
        Optional credential sent as `Authorization: Basic base64(api_key)`
        on state-changing requests only (create/replace). For an Authentik
        forward-auth edge this is `username:app-password`. When set,
        `~/.config/goto/config.yml` contains a secret and is written with
        mode 0600. Prefer {option}`apiKeyFile` so the secret never enters
        the Nix store.
      '';
    };

    apiKeyFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        Like {option}`apiKey`, but the value is read at activation time from
        this file (kept outside git and the Nix store). The rendered
        `~/.config/goto/config.yml` gets mode 0600. Exactly one of the two
        options may be set.
      '';
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = !(cfg.apiKey != null && cfg.apiKeyFile != null);
        message = "programs.gotoLinks: set either apiKey or apiKeyFile, not both.";
      }
    ];

    home.packages = [ cfg.package ];

    xdg.configFile."goto/config.yml" = {
      text =
        ''
          api_url: ${cfg.apiUrl}
          force_replace: ${if cfg.forceReplace then "true" else "false"}
          silent: ${if cfg.silent then "true" else "false"}
          no_browser: ${if cfg.noBrowser then "true" else "false"}
        ''
        + lib.optionalString (cfg.apiKey != null) ''
          api_key: "${cfg.apiKey}"
        '';
    } // lib.optionalAttrs ((cfg.apiKey != null) || (cfg.apiKeyFile != null)) {
      mode = "0600";
    };

    home.activation.gotoApiKeyFile = mkIf (cfg.apiKeyFile != null) (
      lib.hm.dag.entryAfter [ "writeBoundary" ] ''
          cfg="$HOME/.config/goto/config.yml"
          key="$(cat ${lib.escapeShellArg (toString cfg.apiKeyFile)})"
          [[ -n "$key" ]] || { echo "goto: apiKeyFile is empty" >&2; exit 1; }
          # YAML single-quote style: the quote char is doubled
          q="'''"
          escaped="''${key//$q/$q$q}"
          tmp="$(mktemp)"
          grep -v '^api_key:' "$cfg" >"$tmp" || true
          printf "api_key: '%s'\n" "$escaped" >>"$tmp"
          chmod 600 "$tmp"
          mv "$tmp" "$cfg"
        '');
  };
}
