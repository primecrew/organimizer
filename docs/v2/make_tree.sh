#!/bin/bash
# Builds the messy sample tree in $1 (default ./sample) from downloads.
set -e
D=./downloads
T=${1:-./sample}
rm -rf "$T"; mkdir -p "$T"
cd "$T"
mkdir -p "Holiday 2023/Videos" "Phone/DCIM/Camera" "Downloads" "Documents" "Already sorted/March 2024"
cp $D/picsum_10.jpg "IMG_20240315_142530.jpg"
convert $D/picsum_20.jpg "Screenshot 2024-03-15 at 10.22.01.png"
cp $D/picsum_30.jpg "photo (1).JPG"
cp $D/picsum_40.jpg "photo (2).JPG"
cp $D/picsum_50.jpg "photo.JPG"
echo "shopping list" > "notes.txt"
cp /usr/share/doc/*/copyright /dev/null 2>/dev/null || true
printf '%%PDF-1.4\n1 0 obj<<>>endobj\ntrailer<<>>\n%%%%EOF\n' > "receipt.pdf"
cp $D/wm_1.JPG "Holiday 2023/DSC_0001.JPG"
cp $D/wm_2.jpg "Holiday 2023/DSC_0002.jpg"
cp $D/wm_3.jpg "Holiday 2023/été à Paris – copy.jpg"
cp $D/wm_4.jpg "Holiday 2023/IMG_20230704_101500.jpg"
echo '<x:xmpmeta/>' > "Holiday 2023/DSC_0001.JPG.xmp"
echo '{"title":"DSC_0002"}' > "Holiday 2023/DSC_0002.jpg.json"
cp $D/sample1.mp4 "Holiday 2023/Videos/clip.MP4"
cp $D/sample2.mp4 "Phone/DCIM/Camera/VID_20240101_120000.mp4"
cp $D/wm_6.JPG "Phone/DCIM/Camera/IMG_20240101_120500.jpg"
cp $D/picsum_60.jpg "Phone/DCIM/Camera/IMG_20240101_120500 (1).jpg"
convert $D/picsum_10.jpg "Downloads/image.png"
cp "Downloads/image.png" "Downloads/disguised_png.jpg"
cp $D/picsum_20.jpg "Downloads/wallpaper   final__v2..jpg"
echo "readme" > "Downloads/README.txt"
cp receipt.pdf "Documents/report.pdf"
echo "todo" > "Documents/todo.txt"
cp $D/picsum_30.jpg "Already sorted/March 2024/IMG_20240320_090000.jpg"
# deterministic mtimes
find . -type f -exec touch -d "2022-11-05 12:00:00" {} +
touch -d "2021-06-01 08:00:00" "Downloads/image.png" "Downloads/disguised_png.jpg"
touch -d "2020-02-29 18:30:00" "Downloads/wallpaper   final__v2..jpg" "photo.JPG" "photo (1).JPG" "photo (2).JPG"
