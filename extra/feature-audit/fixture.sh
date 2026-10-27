printf '\033[2J\033[H'
i=1
while [ "$i" -le 80 ]; do
  case "$i" in
    10|60) printf 'AUDIT_FIND_NEEDLE_314159 at line %s\n' "$i" ;;
    *) printf 'AUDIT_LINE_%03d ordinary log output\n' "$i" ;;
  esac
  i=$((i+1))
done
printf 'AUDIT_COPY_TARGET_271828\nAUDIT_HINT_123456\n/tmp/rio-feature-audit/open-target.txt\n'
