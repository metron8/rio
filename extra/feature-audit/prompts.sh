printf '\033[2J\033[H'
for group in 1 2 3; do
    printf '\033]133;A\007AUDIT_PROMPT_%s\n' "$group"
    i=1
    while [ "$i" -le 35 ]; do printf 'GROUP_%s output_%02d\n' "$group" "$i"; i=$((i+1)); done
done
