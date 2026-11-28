#!/bin/sh
# Resets Texcel to its default settings by deleting its settings file, for troubleshooting.
# Work kept for recovery after a crash (the recovery folder) is left alone.
set -u

dir="$HOME/Library/Application Support/Texcel"
file="$dir/settings.json"

# A running Texcel writes its settings again when it quits, so it must be closed first.
if pgrep -x Texcel >/dev/null 2>&1; then
    echo "Texcel is running. Quit Texcel, then run this again."
    echo "Texcelが起動しています。Texcelを終了してから、もう一度実行してください。"
    exit 1
fi

if [ ! -e "$file" ]; then
    echo "There is no settings file; Texcel already uses its default settings."
    echo "設定ファイルはありません。Texcelはすでに初期設定で動きます。"
    exit 0
fi

echo "This deletes the Texcel settings file / Texcelの設定ファイルを削除します:"
echo "  $file"
printf "Delete it? / 削除しますか？ [y/N]: "
read -r answer
case "$answer" in
    y | Y) ;;
    *)
        echo "Nothing was deleted. / 削除しませんでした。"
        exit 0
        ;;
esac

# Leftover temporary files of an interrupted settings save go too.
rm -f "$file" "$dir"/.texcel-settings-*.tmp
echo "Deleted. Texcel starts with its default settings next time."
echo "削除しました。次回の起動からTexcelは初期設定で動きます。"
