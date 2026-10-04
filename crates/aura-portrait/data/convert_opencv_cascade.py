#!/usr/bin/env python3
"""Convert an OpenCV Haar cascade (the `opencv_storage` XML format) into AURA's line format.

Standard library only, so the conversion can be re-run anywhere the repository is checked
out. The output is read by `crates/aura-portrait/src/cascade.rs`.

    python3 convert_opencv_cascade.py haarcascade_frontalface_alt2.xml frontalface_alt2.cascade

The format is deliberately boring:

    # comment lines (provenance and the licence) start with '#'
    cascade <window width> <window height> <stages> <features>
    s <stage threshold> <tree count>
    t <node count> (<left> <right> <feature> <threshold>) x nodes  (<leaf value>) x (nodes + 1)
    f <rect count> (<x> <y> <w> <h> <weight>) x rects

Numbers are copied as text from the XML, never re-printed, so the converted file carries
exactly the values OpenCV reads and the conversion cannot round anything.
"""

import sys
import xml.etree.ElementTree as ET


def words(node):
    return (node.text or "").split()


def main(source, target):
    raw = open(source, encoding="utf-8").read()
    licence = raw.split("<opencv_storage>")[0]
    licence = licence.replace('<?xml version="1.0"?>', "").replace("<!--", "").replace("-->", "")
    root = ET.fromstring(raw[raw.index("<opencv_storage>"):])
    cascade = root.find("cascade")
    if cascade.findtext("featureType").strip() != "HAAR":
        raise SystemExit("only HAAR cascades are supported")
    width = int(cascade.findtext("width"))
    height = int(cascade.findtext("height"))
    stages = cascade.find("stages").findall("_")
    features = cascade.find("features").findall("_")

    out = []
    out.append(f"# Converted from {source.split('/')[-1]} by convert_opencv_cascade.py.")
    out.append("# The licence below is reproduced verbatim from the source file.")
    for line in licence.strip("\n").splitlines():
        out.append(("# " + line).rstrip())
    out.append(f"cascade {width} {height} {len(stages)} {len(features)}")
    for stage in stages:
        trees = stage.find("weakClassifiers").findall("_")
        out.append(f"s {stage.findtext('stageThreshold').strip()} {len(trees)}")
        for tree in trees:
            nodes = words(tree.find("internalNodes"))
            leaves = words(tree.find("leafValues"))
            count = len(nodes) // 4
            if len(nodes) % 4 != 0 or len(leaves) != count + 1:
                raise SystemExit("unexpected tree shape")
            out.append("t " + " ".join([str(count)] + nodes + leaves))
    for feature in features:
        if (feature.findtext("tilted") or "0").strip() != "0":
            raise SystemExit("tilted features are not supported")
        rects = [words(r) for r in feature.find("rects").findall("_")]
        flat = []
        for rect in rects:
            weight = rect[4].rstrip(".")
            flat.extend(rect[:4] + [weight if weight not in ("", "-") else rect[4]])
        out.append("f " + " ".join([str(len(rects))] + flat))
    with open(target, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(out) + "\n")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
