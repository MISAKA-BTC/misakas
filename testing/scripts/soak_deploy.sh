#!/usr/bin/env bash
# Put a candidate binary on ONE public testnet node, or take it back off.
#
# One node per invocation, on purpose. A script that can update the fleet in one command is a
# script that can break the fleet in one command, and the fleet here is a live network serving
# real peers.
#
# Usage:
#   soak_deploy.sh install  <ssh-target> <unit> <binary> <sha256>
#   soak_deploy.sh rollback <ssh-target> <unit>
#   soak_deploy.sh verify   <ssh-target> <unit>
#
# `install` keeps the running binary as <path>.prev before replacing it, so `rollback` has
# something to restore. It refuses if a .prev already exists — that means a previous install was
# never rolled back or confirmed, and overwriting it would destroy the only way back.
set -uo pipefail

ACTION=${1:?usage: soak_deploy.sh install|rollback|verify <ssh-target> <unit> [binary] [sha256]}
TARGET=${2:?ssh-target required}
UNIT=${3:?systemd unit required}
SSH="ssh -i ${SSH_KEY:-$HOME/.ssh/claude_key} -o BatchMode=yes -o ConnectTimeout=15"

exe_path() {
  $SSH "$TARGET" "systemctl show -p ExecStart --value '$UNIT' 2>/dev/null | sed -n 's/.*path=\([^ ;]*\).*/\1/p' | head -1"
}

case "$ACTION" in
  verify)
    $SSH "$TARGET" "
      p=\$(systemctl show -p ExecStart --value '$UNIT' | sed -n 's/.*path=\([^ ;]*\).*/\1/p' | head -1)
      echo \"unit:    $UNIT (\$(systemctl is-active '$UNIT'))\"
      echo \"binary:  \$p\"
      echo \"sha256:  \$(sha256sum \$p 2>/dev/null | cut -d' ' -f1)\"
      echo \"prev:    \$([ -f \$p.prev ] && sha256sum \$p.prev | cut -d' ' -f1 || echo '(none)')\"
      echo \"running: \$(readlink -f /proc/\$(systemctl show -p MainPID --value '$UNIT')/exe 2>/dev/null || echo '(not running)')\"
    "
    ;;

  install)
    BIN=${4:?local binary path required}
    WANT=${5:?expected sha256 required}
    [ "$(sha256sum "$BIN" | cut -d' ' -f1)" = "$WANT" ] || { echo "local binary does not match the expected sha256" >&2; exit 1; }

    path=$(exe_path)
    [ -n "$path" ] || { echo "could not determine the unit's binary path" >&2; exit 1; }

    # Refuse to bury an unresolved previous install.
    if $SSH "$TARGET" "[ -f '$path.prev' ]"; then
      echo "REFUSING: $path.prev already exists on $TARGET." >&2
      echo "A previous install was never rolled back or confirmed. Overwriting it would destroy" >&2
      echo "the only way back. Resolve it first (confirm and remove .prev, or roll back)." >&2
      exit 1
    fi

    echo "installing on $TARGET: $path"
    scp -i "${SSH_KEY:-$HOME/.ssh/claude_key}" -o BatchMode=yes "$BIN" "$TARGET:$path.new" || exit 1
    $SSH "$TARGET" "
      set -e
      got=\$(sha256sum '$path.new' | cut -d' ' -f1)
      [ \"\$got\" = '$WANT' ] || { echo \"transfer corrupted: \$got\"; rm -f '$path.new'; exit 1; }
      cp -p '$path' '$path.prev'
      systemctl stop '$UNIT'
      mv '$path.new' '$path'
      chmod +x '$path'
      systemctl start '$UNIT'
      sleep 10
      systemctl is-active '$UNIT'
    " || { echo "install failed on $TARGET — run: soak_deploy.sh rollback $TARGET $UNIT" >&2; exit 1; }
    ;;

  rollback)
    path=$(exe_path)
    $SSH "$TARGET" "
      set -e
      [ -f '$path.prev' ] || { echo 'no .prev to restore — nothing to roll back'; exit 1; }
      systemctl stop '$UNIT'
      mv '$path.prev' '$path'
      chmod +x '$path'
      systemctl start '$UNIT'
      sleep 10
      systemctl is-active '$UNIT'
      echo \"restored: \$(sha256sum '$path' | cut -d' ' -f1)\"
    "
    ;;

  *) echo "unknown action: $ACTION" >&2; exit 1;;
esac
