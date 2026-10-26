---
diagrams:
  - kind: illustration
    id: fig-add-inputs
    image: assets/fig-add-inputs/figure.svg
    media-type: image/svg+xml
    width: 65mm
    caption: "add : Int → Int → Int"
    description: Three separate blue squares labelled Int, connected left to right by two arrows.

  - kind: comic
    id: fig-choosing-a-result
    image: assets/fig-choosing-a-result/strip.png
    media-type: image/png
    caption: Alice and Bot separate a Boolean question from the result selected by match.
    description: >-
      Four panels under an ash tree. Alice asks whether 20 + 22 == 42 makes
      42. Bot explains that equality gives a Bool. They trace the equality
      to true, then match true with true => 42 and false => 0 to select 42.
      Changing the equality to 20 + 22 == 41 gives false and selects 0.
    width: 170mm
---

# Saved artwork preparation

```{#callout-proof .callout}
Only original source is editable.
```

```{#fig-add-inputs .illustration}
Show three separate, equally sized blue squares in one horizontal row,
with a right-pointing arrow between each neighbouring pair. Put Int inside
each square. Keep the full function signature in the caption, not above
the artwork. Use rough hand-drawn contours and Excalifont lettering. Keep
the fills exact and protect the lettering from the stroke filter.
```

```{#fig-choosing-a-result .comic}
Show Alice and Bot in four pencil-sketch panels beneath an ash tree.
Alice asks what 20 + 22 == 42 produces. Bot distinguishes the Boolean
question from the selected integer. Trace the equality to true, then show
both match arms and select true => 42. Finally change the right side of the
equality to 41, obtaining false and selecting false => 0. Keep character
identity from the shared cast sheet; use loose boxes and arrows for the
supporting explanation. Preserve exact code characters and readable dialogue.
```
