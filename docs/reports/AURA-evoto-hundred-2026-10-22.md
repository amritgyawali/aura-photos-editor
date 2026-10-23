# A hundred Evoto-style tools on 27 real photographs - 2026-10-22

Photographs: 27 Canon CR3 files of a Nepali wedding (`H:\AURA EDIT`), imported by reference into an isolated catalogue; the originals were only read. Every tool was used two ways: through the application's own commands on the photographs that suit it (`crates/aura-app/tests/evoto100_photos.rs`), and by its control in the real desktop window (`scripts/test-evoto100-gui.py`). Each before-and-after was then looked at by eye.

**What "changed" and "inside" mean.** *Changed* is the share of the frame that moved by three levels of 255 or more. *Inside* is the share of all the change that landed in the region the tool is for (faces for face tools, the background for background tools); 1.00 means none leaked out.

**On "the same as Evoto".** Evoto's models and code were not opened or copied, and no Evoto output of these photographs was available, so sameness cannot be measured. Each tool is graded on whether it does its job well on these photographs.

**Grades:** good: 87, usable: 12, weak: 1.

## A face: eyes, nose, cheeks

| # | Tool | Window | Engine (photo: changed, inside, ms) | Grade | What was seen |
|---|---|---|---|---|---|
| A1 | Eye height | works | 989A2698.CR3: 0.6%, 1.00, 752<br>989A2699.CR3: 0.6%, 1.00, 828<br>989A2703.CR3: 0.5%, 1.00, 785 | **good** | Both eyes enlarged or reduced in place; on closed eyes the lids and lashes move together, no tearing. |
| A2 | Eye tilt | works | 989A2698.CR3: 0.4%, 1.00, 157<br>989A2699.CR3: 0.4%, 1.00, 160<br>989A2703.CR3: 0.3%, 1.00, 153 | **good** | Outer corners rotate up or down about each eye's centre; subtle at 100, natural. |
| A3 | Inner corners | works, small change | 989A2698.CR3: 0.1%, 1.00, 161<br>989A2699.CR3: 0.0%, 1.00, 181<br>989A2703.CR3: 0.0%, 1.00, 172 | **good** | Moves only the inner corners, a few pixels; the bridge of the nose is untouched. |
| A4 | Outer corners | works | 989A2698.CR3: 0.1%, 1.00, 155<br>989A2699.CR3: 0.1%, 1.00, 160<br>989A2703.CR3: 0.1%, 1.00, 171 | **good** | Moves only the outer corners; wing liner moves with them. |
| A5 | Eye symmetry | works, small change | 989A2698.CR3: 0.0%, 1.00, 190<br>989A2699.CR3: 0.0%, 1.00, 167<br>989A2703.CR3: 0.0%, 1.00, 148 | **good** | Measures the two openings and evens the smaller toward the larger; changed nothing on symmetric closed eyes (correct), acted on 2698's slightly uneven pair. |
| A6 | Nose bridge | works | 989A2698.CR3: 0.1%, 1.00, 186<br>989A2699.CR3: 0.1%, 1.00, 187<br>989A2703.CR3: 0.1%, 1.00, 175 | **good** | Narrows the bridge between the eyes without bending the nose's outline. |
| A7 | Nose tip | works, small change | 989A2698.CR3: 0.0%, 1.00, 174<br>989A2699.CR3: 0.0%, 1.00, 190<br>989A2703.CR3: 0.0%, 1.00, 168 | **good** | Shrinks the tip alone; nostrils and lip untouched. |
| A8 | Nose lift | works | 989A2698.CR3: 0.1%, 1.00, 180<br>989A2699.CR3: 0.1%, 1.00, 194<br>989A2703.CR3: 0.1%, 1.00, 170 | **good** | Lifts the tip; the change follows the nose's lower edge. |
| A9 | Nostrils | works, small change | 989A2698.CR3: 0.0%, 1.00, 194<br>989A2699.CR3: 0.0%, 1.00, 175<br>989A2703.CR3: 0.0%, 1.00, 198 | **good** | Narrows the wings of the nose only. |
| A10 | Cheek apples | works | 989A2698.CR3: 0.5%, 1.00, 174<br>989A2699.CR3: 0.4%, 1.00, 180<br>989A2703.CR3: 0.3%, 1.00, 188 | **good** | Fuller apples that also move the face's outline out slightly, as a fuller cheek does. |

## B face: mouth, brows, shape

| # | Tool | Window | Engine (photo: changed, inside, ms) | Grade | What was seen |
|---|---|---|---|---|---|
| B1 | Upper lip | works | 989A2698.CR3: 0.1%, 1.00, 164<br>989A2699.CR3: 0.1%, 1.00, 189<br>989A2703.CR3: 0.1%, 1.00, 182 | **good** | Fuller upper lip on the measured mouth; after the parse fix it is centred on 2698's lips, the half in shadow included. |
| B2 | Lower lip | works | 989A2698.CR3: 0.1%, 1.00, 205<br>989A2699.CR3: 0.1%, 1.00, 190<br>989A2703.CR3: 0.1%, 1.00, 201 | **good** | Fuller lower lip, centred; no smear into the chin. |
| B3 | Mouth position | works | 989A2698.CR3: 0.3%, 1.00, 320<br>989A2699.CR3: 0.2%, 1.00, 182<br>989A2703.CR3: 0.2%, 1.00, 181 | **good** | Raises or lowers the whole mouth smoothly. |
| B4 | Brow height | works | 989A2698.CR3: 0.3%, 1.00, 177<br>989A2699.CR3: 0.3%, 1.00, 171<br>989A2703.CR3: 0.3%, 1.00, 172 | **good** | Raises both brows; skin above them compresses naturally. |
| B5 | Brow arch | works | 989A2698.CR3: 0.1%, 1.00, 166<br>989A2699.CR3: 0.1%, 1.00, 168<br>989A2703.CR3: 0.1%, 1.00, 169 | **good** | Lifts the tail of each brow into an arch. |
| B6 | Brow thickness | works | 989A2698.CR3: 0.1%, 1.00, 159<br>989A2699.CR3: 0.1%, 1.00, 182<br>989A2703.CR3: 0.1%, 1.00, 176 | **good** | Thickens each brow along its own length. |
| B7 | Temples | works | 989A2698.CR3: 0.5%, 1.00, 173<br>989A2699.CR3: 0.4%, 1.00, 203<br>989A2703.CR3: 0.4%, 1.00, 192 | **good** | Fills or slims the temples; the hairline stays put. |
| B8 | Lower face length | works | 989A2698.CR3: 0.9%, 1.00, 178<br>989A2699.CR3: 0.9%, 1.00, 173<br>989A2703.CR3: 0.8%, 1.00, 169 | **good** | Shortens or lengthens the face below the nose, lips and chin together. |
| B9 | Chin width | works | 989A2698.CR3: 0.1%, 1.00, 172<br>989A2699.CR3: 0.1%, 1.00, 184<br>989A2703.CR3: 0.1%, 1.00, 174 | **usable** | Works on the measured chin; deliberately gentle, about 0.1% of the frame at 100. Strong reshaping needs Liquify. |
| B10 | Hair volume | works | 989A2698.CR3: 2.6%, 1.00, 184<br>989A2699.CR3: 1.4%, 1.00, 188<br>989A2703.CR3: 1.5%, 1.00, 186 | **good** | Lifts the hair above the crown; the tiara moves with it, no tearing at the hair edge. |

## C body

| # | Tool | Window | Engine (photo: changed, inside, ms) | Grade | What was seen |
|---|---|---|---|---|---|
| C1 | Thighs | works | 989A3870.CR3: 2.1%, -, 233<br>989A3872.CR3: 3.6%, -, 244<br>989A3839.CR3: 4.6%, -, 1006 | **good** | Slims both thighs on the person under the chosen face; neighbours in a group are left alone. |
| C2 | Calves | works | 989A3870.CR3: 1.3%, -, 232<br>989A3872.CR3: 2.5%, -, 234<br>989A3839.CR3: 3.2%, -, 240 | **good** | Slims the calves of that person only. |
| C3 | Flat stomach | works | 989A3870.CR3: 1.9%, -, 230<br>989A3872.CR3: 3.0%, -, 223<br>989A3839.CR3: 4.4%, -, 207 | **good** | Pulls the stomach in on the side the silhouette shows it. |
| C4 | Hip lift | works | 989A3870.CR3: 2.4%, -, 230<br>989A3872.CR3: 3.4%, -, 220<br>989A3839.CR3: 4.4%, -, 220 | **good** | Lifts the hip line; fabric pattern bends smoothly. |
| C5 | Shoulder line | works | 989A3870.CR3: 1.2%, -, 223<br>989A3872.CR3: 1.9%, -, 222<br>989A3839.CR3: 2.4%, -, 206 | **good** | Lowers or raises the shoulder line of that person. |
| C6 | Torso length | works | 989A3870.CR3: 31.8%, -, 216<br>989A3872.CR3: 45.5%, -, 214<br>989A3839.CR3: 39.7%, -, 207 | **usable** | Lengthens the torso smoothly; moves a third of the frame, so check the background lines it stretches. |
| C7 | Height | works | 989A3870.CR3: 32.1%, -, 226<br>989A3872.CR3: 45.9%, -, 213<br>989A3839.CR3: 40.8%, -, 216 | **usable** | Taller legs and body; as with any height tool, straight lines behind the person bend - use it on plain backgrounds. |
| C8 | Neck slim | works | 989A2698.CR3: 0.3%, -, 228<br>989A2699.CR3: 0.3%, -, 216<br>989A2703.CR3: 0.2%, -, 225 | **good** | Slims the neck below the measured chin; no longer reaches the necklace. |
| C9 | Double chin | works | 989A2698.CR3: 0.2%, -, 222<br>989A2699.CR3: 0.2%, -, 221<br>989A2703.CR3: 0.2%, -, 211 | **good** | Tightens and lightens the fold under the measured chin of a face looking down. |
| C10 | Level head | works | 989A2698.CR3: 9.2%, -, 209<br>989A2699.CR3: 8.9%, -, 213<br>989A2703.CR3: 8.0%, -, 220 | **good** | Rotates the head about the neck to level the measured eye line; the background is not rotated. |

## D skin

| # | Tool | Window | Engine (photo: changed, inside, ms) | Grade | What was seen |
|---|---|---|---|---|---|
| D1 | Aegyo-sal | works | 989A2698.CR3: 0.1%, 1.00, 140<br>989A2699.CR3: 0.1%, 1.00, 131<br>989A2703.CR3: 0.1%, 1.00, 136 | **good** | A soft lift under each eye; invisible at low values, natural at 100. |
| D2 | Lip lines | works | 989A2698.CR3: 0.1%, 1.00, 142<br>989A2699.CR3: 0.0%, 1.00, 151<br>989A2703.CR3: 0.0%, 1.00, 139 | **good** | Softens vertical lip creases inside the lips only. |
| D3 | Dewy glow | works | 989A2698.CR3: 0.6%, 1.00, 150<br>989A2699.CR3: 0.6%, 1.00, 182<br>989A2703.CR3: 0.5%, 1.00, 162 | **good** | Glow on forehead, nose bridge and cheek tops; visible at ordinary strengths after the fix. |
| D4 | Stubble shadow | works | 989A2892.CR3: 0.1%, 1.00, 732<br>989A3842.CR3: 0.3%, 1.00, 187<br>989A3839.CR3: 0.2%, 1.00, 199 | **usable** | Evens stubble shadow toward that person's cheeks on the two men the parse finds a beard on. It also touched an older woman's shaded jaw at 0.2%, which the parse called facial hair. |
| D5 | Rosy complexion | works | 989A2698.CR3: 2.5%, 1.00, 156<br>989A2699.CR3: 2.1%, 1.00, 163<br>989A2703.CR3: 2.0%, 1.00, 163 | **good** | Rotates each person's own skin colour slightly toward rose, luminance unchanged; no ideal colour. |
| D6 | Even teeth | works, small change | 989A2698.CR3: 0.0%, 1.00, 161<br>989A2699.CR3: 0.0%, 1.00, 168<br>989A2703.CR3: 0.0%, 1.00, 162 | **good** | Evens the teeth's colour; correctly did nothing on these closed mouths. |
| D7 | Sharpen features | works | 989A2698.CR3: 0.4%, 1.00, 176<br>989A2699.CR3: 0.4%, 1.00, 169<br>989A2703.CR3: 0.3%, 1.00, 172 | **good** | Sharpens eyes, brows and lips only. |
| D8 | Beard definition | works, small change | 989A2892.CR3: 0.2%, 1.00, 175<br>989A3842.CR3: 0.1%, 1.00, 156<br>989A3839.CR3: 0.6%, 1.00, 149 | **usable** | Defines a full beard well (2892). A moustache or goatee alone is below the parse's beard threshold and gets nothing. |
| D9 | Clothing colour on skin | works | 989A2698.CR3: 1.0%, 0.87, 164<br>989A2699.CR3: 1.5%, 0.88, 154<br>989A2703.CR3: 1.0%, 0.90, 166 | **good** | Removes red cast from the sari on the face and neck along the measured direction; 13% of the change is just outside the parse's skin. |
| D10 | Refine pores | works, small change | 989A2698.CR3: 0.0%, 1.00, 165<br>989A2699.CR3: 0.0%, 1.00, 161<br>989A2703.CR3: 0.0%, 1.00, 163 | **usable** | Refines pores only (refuses anything deeper); on these already smooth, made-up faces the change is barely visible. |

## E makeup

| # | Tool | Window | Engine (photo: changed, inside, ms) | Grade | What was seen |
|---|---|---|---|---|---|
| E1 | Eyeliner | works, small change | 989A2698.CR3: 0.0%, 1.00, 185<br>989A2699.CR3: 0.0%, 1.00, 155<br>989A2703.CR3: 0.0%, 1.00, 147 | **good** | Liner on the measured lash line, thicker at the outer corner. Fixed: on closed eyes it had crossed the middle of the lid. |
| E2 | Eyeliner wing | works, small change | 989A2698.CR3: 0.0%, 1.00, 152<br>989A2699.CR3: 0.0%, 1.00, 153<br>989A2703.CR3: 0.0%, 0.98, 137 | **good** | A short wing from the outer corner, drawn together with the liner. Set alone it draws nothing, by design; the window test confirmed this. |
| E3 | Fuller lashes | works, small change | 989A2698.CR3: 0.0%, 1.00, 145<br>989A2699.CR3: 0.0%, 1.00, 124<br>989A2703.CR3: 0.0%, 1.00, 141 | **good** | Deepens the existing lashes; on closed eyes it follows the lashes down from the lid edge. |
| E4 | Lower lashes | works, small change | 989A2698.CR3: 0.0%, 1.00, 128<br>989A2699.CR3: 0.0%, 1.00, 132<br>989A2703.CR3: 0.0%, 1.00, 132 | **good** | Defines lower lashes on open eyes; harmless on closed eyes (it only darkens what is already dark). |
| E5 | Lip liner | works | 989A2698.CR3: 0.1%, 1.00, 136<br>989A2699.CR3: 0.1%, 1.00, 145<br>989A2703.CR3: 0.1%, 1.00, 146 | **usable** | Liner around the lips, now centred. At 100 on 2698 a short line still runs under the right corner where blush meets the lip. |
| E6 | Ombre lips | works | 989A2698.CR3: 0.1%, 1.00, 147<br>989A2699.CR3: 0.1%, 1.00, 135<br>989A2703.CR3: 0.1%, 1.00, 161 | **good** | Deeper centre fading to the edges of the lips; centred after the parse fix. |
| E7 | Highlighter | works | 989A2698.CR3: 0.1%, 1.00, 160<br>989A2699.CR3: 0.1%, 1.00, 149<br>989A2703.CR3: 0.1%, 1.00, 171 | **good** | Highlights cheekbones, nose bridge and brow bone on skin only. |
| E8 | Bronzer | works | 989A2698.CR3: 0.5%, 0.99, 155<br>989A2699.CR3: 0.4%, 0.99, 165<br>989A2703.CR3: 0.3%, 0.99, 157 | **good** | Warms the face's edges and under the cheekbones; stays on skin. |
| E9 | Enhance makeup | works | 989A2698.CR3: 0.2%, 1.00, 167<br>989A2699.CR3: 0.1%, 1.00, 166<br>989A2703.CR3: 0.1%, 1.00, 151 | **good** | Strengthens the makeup already worn (lips, lids, blush) without adding colour where there was none. |
| E10 | Fill brows | works | 989A2698.CR3: 0.1%, 1.00, 169<br>989A2699.CR3: 0.1%, 0.99, 169<br>989A2703.CR3: 0.1%, 1.00, 166 | **good** | Fills gaps in sparse brows with their own darkest hairs' colour. |

## F hair and body

| # | Tool | Window | Engine (photo: changed, inside, ms) | Grade | What was seen |
|---|---|---|---|---|---|
| F1 | Hair shine | works | 989A2698.CR3: 1.8%, 1.00, 155<br>989A2699.CR3: 1.4%, 1.00, 154<br>989A2703.CR3: 5.6%, 1.00, 170 | **good** | Shine along the hair's lit strands; crisp hair edge, no halo against the background. |
| F2 | Fill parting | works | 989A2698.CR3: 0.1%, 1.00, 198<br>989A2699.CR3: 0.0%, 1.00, 187<br>989A2703.CR3: 0.0%, 1.00, 164 | **good** | Darkens a visible scalp line in the parting; tiny area, as it should be. |
| F3 | Hair colour depth | works | 989A2698.CR3: 2.2%, 1.00, 172<br>989A2699.CR3: 2.0%, 1.00, 169<br>989A2703.CR3: 8.4%, 1.00, 188 | **good** | Richer colour in the body of the hair; lit strands no longer turn orange. |
| F4 | Calm hair edge | works, small change | 989A2698.CR3: 0.0%, 1.00, 175<br>989A2699.CR3: 0.2%, 1.00, 164<br>989A2703.CR3: 0.2%, 1.00, 181 | **usable** | Calms frizz at the hair's outline against the background; nothing to do on 2698 (hair against the veil), 0.2% on the others. |
| F5 | Body definition | works | 989A3839.CR3: 0.9%, 0.81, 182<br>989A2890.CR3: 2.9%, 0.89, 825<br>989A3272.CR3: 0.3%, 0.83, 832 | **usable** | Defines arms and shoulders, now only on skin the colour of the face; 19% of its change landed outside body skin on the crowded group frame. |
| F6 | Soften body hair | works, works, small change | 989A3839.CR3: 0.0%, -, 122<br>989A2890.CR3: 0.0%, -, 126<br>989A3272.CR3: 0.0%, -, 139 | **usable** | Softens fine hair on skin it is placed over; these photographs show almost no visible body hair, so the change is near zero. |
| F7 | Smooth cellulite | works | 989A3839.CR3: 0.2%, -, 135<br>989A2890.CR3: 0.1%, -, 115<br>989A3272.CR3: 0.1%, -, 134 | **good** | Smooths uneven texture where placed, keeping the skin's grain. |
| F8 | Even tan lines | works, small change | 989A3839.CR3: 0.2%, -, 121<br>989A2890.CR3: 0.2%, -, 129<br>989A3272.CR3: 0.1%, -, 140 | **good** | Evens a tan line toward the lighter side where placed. |
| F9 | Rejuvenate hands | works | 989A3839.CR3: 0.1%, -, 121<br>989A2890.CR3: 0.1%, -, 145<br>989A3272.CR3: 0.0%, -, 127 | **good** | Softens veins and wrinkles on the back of a hand, texture kept. |
| F10 | Even knuckles and elbows | works | 989A3839.CR3: 0.1%, -, 121<br>989A2890.CR3: 0.0%, -, 140<br>989A3272.CR3: 0.0%, -, 132 | **good** | Lightens dark knuckles and elbows toward the surrounding skin. |

## G attire

| # | Tool | Window | Engine (photo: changed, inside, ms) | Grade | What was seen |
|---|---|---|---|---|---|
| G1 | Gold jewellery | works | 989A2698.CR3: 3.5%, 1.00, 853<br>989A2699.CR3: 3.3%, 1.00, 892<br>989A2703.CR3: 3.6%, 1.00, 864 | **good** | Richer, cleaner gold on jewellery and zari; skin of the same hue is not touched. |
| G2 | Recover red fabric | works | 989A3150.CR3: 0.9%, -, 918<br>989A2704.CR3: 0.1%, -, 737<br>989A2708.CR3: 0.2%, -, 754 | **usable** | Rebuilds weave where red really clipped (the red curtain in 3150). Fixed: it had measured clipping against the whole frame and never acted. These CR3s rarely clip red, so the change is small. |
| G3 | Veil clarity | works | 989A2706.CR3: 1.7%, 0.99, 663<br>989A2705.CR3: 0.8%, 0.97, 606<br>989A2698.CR3: 3.0%, 0.99, 752 | **good** | Removes the veil's added light from the face behind it: a clear face under a red net, no dehaze contrast. |
| G4 | Embroidery detail | works | 989A2698.CR3: 10.9%, 1.00, 153<br>989A2699.CR3: 8.6%, 1.00, 715<br>989A2703.CR3: 12.4%, 1.00, 735 | **good** | Brings out embroidery and zari texture on the garment only. |
| G5 | Smooth garment edges | works | 989A2698.CR3: 1.7%, 1.00, 150<br>989A2699.CR3: 2.0%, 1.00, 164<br>989A2703.CR3: 3.2%, 1.00, 183 | **good** | Smooths jagged garment outlines against the background. |
| G6 | Deepen mehndi | works | 989A3839.CR3: 1.6%, 0.82, 1018<br>989A2890.CR3: 0.7%, 0.80, 881<br>989A3272.CR3: 0.4%, 0.79, 903 | **usable** | Deepens mehndi where the hands show it; little visible mehndi in the frames tried. |
| G7 | Clean white clothes | works | 989A3150.CR3: 1.9%, 0.99, 830<br>989A2892.CR3: 3.0%, 1.00, 762<br>989A2698.CR3: 0.8%, 1.00, 733 | **good** | Removes the cast from a white daura suruwal, skin unchanged. |
| G8 | Dark suit detail | works | 989A2933.CR3: 2.2%, 1.00, 577<br>989A2890.CR3: 7.1%, 1.00, 200<br>989A2698.CR3: 0.0%, 1.00, 188 | **good** | Opens the shadows of a dark suit to show its fabric; the shirt is unchanged. |
| G9 | Outfit colour | works | 989A2698.CR3: 23.0%, 1.00, 172<br>989A2699.CR3: 18.0%, 1.00, 733<br>989A2703.CR3: 25.5%, 0.99, 755 | **good** | Changes the outfit's hue as one fabric, embroidery included; skin untouched. |
| G10 | Glasses reflection colour | works | 989A2698.CR3: 0.1%, -, 136<br>989A2699.CR3: 0.1%, -, 136<br>989A2703.CR3: no eyes found to place it on | **good** | Neutralises the coloured coating reflection on glasses where placed. |

## H background

| # | Tool | Window | Engine (photo: changed, inside, ms) | Grade | What was seen |
|---|---|---|---|---|---|
| H1 | Backdrop light | works | 989A2698.CR3: 13.4%, 1.00, 162<br>989A2699.CR3: 15.7%, 0.95, 173<br>989A2703.CR3: 9.9%, 0.91, 173 | **good** | A soft light behind the subject, from the learned person outline. |
| H3 | Background saturation | works | 989A2698.CR3: 49.5%, 0.96, 154<br>989A2699.CR3: 52.8%, 0.97, 171<br>989A2703.CR3: 39.6%, 0.97, 154 | **good** | Desaturates or saturates the background only; the bride and her veil are unchanged. |
| H4 | Background brightness | works | 989A2698.CR3: 50.1%, 0.98, 172<br>989A2699.CR3: 53.6%, 0.98, 153<br>989A2703.CR3: 39.1%, 0.99, 164 | **good** | Brightens or darkens the background only. |
| H5 | Background warmth | works | 989A2698.CR3: 49.2%, 0.98, 164<br>989A2699.CR3: 52.9%, 0.98, 176<br>989A2703.CR3: 35.3%, 0.99, 158 | **good** | Warms or cools the background only. |
| H6 | Depth blur | works | 989A2698.CR3: 43.5%, 0.99, 515<br>989A2699.CR3: 33.2%, 0.99, 516<br>989A2703.CR3: 28.8%, 0.99, 488 | **good** | Blur that grows with distance from the subject; clean edge at the veil. |
| H8 | Background noise | works | 989A2698.CR3: 0.1%, 0.99, 307<br>989A2699.CR3: 0.1%, 0.99, 340<br>989A2703.CR3: 0.1%, 0.98, 305 | **good** | Reduces noise in the background only; these frames are clean, so it is small. |
| H10 | Light wrap | works | 989A2698.CR3: 6.3%, 0.46, 272<br>989A2699.CR3: 6.5%, 0.30, 250<br>989A2703.CR3: 11.0%, 0.27, 257 | **good** | Wraps the background's light onto the subject's edge, as intended (so its change is on the subject's rim). |
| H2 | Colour, keep shadows | works | 989A3870.CR3: 50.7%, 0.88, 1176<br>989A3872.CR3: 50.4%, 0.85, 1197<br>989A3839.CR3: 69.1%, 0.71, 1250 | **good** | Replaces the background with a colour while keeping the shadows people cast; clean group outline. |
| H7 | Sky with clouds | works | 989A2925.CR3: 19.7%, 1.00, 774<br>989A2699.CR3: 22.0%, 1.00, 219<br>989A2709.CR3: 9.4%, 1.00, 691 | **good** | Replaces a blown sky with a blue sky with clouds, under the building's outline. |
| H9 | Remove a cast shadow | works, works, small change | 989A2698.CR3: 0.1%, -, 153<br>989A2699.CR3: 0.1%, -, 131<br>989A2703.CR3: 0.1%, -, 152 | **good** | Lifts a cast shadow where placed, matching the lit surface's colour. |

## I people

| # | Tool | Window | Engine (photo: changed, inside, ms) | Grade | What was seen |
|---|---|---|---|---|---|
| I1 | People exposure | works | 989A3870.CR3: 60.2%, 0.98, 183<br>989A3872.CR3: 59.7%, 0.97, 198<br>989A3839.CR3: 50.9%, 0.84, 180 | **good** | Brightens the people only; the background stays as it was. |
| I2 | People warmth | works | 989A3870.CR3: 54.5%, 0.98, 154<br>989A3872.CR3: 53.6%, 0.97, 173<br>989A3839.CR3: 33.3%, 0.84, 148 | **good** | Warms the people only. |
| I3 | People clarity | works | 989A3870.CR3: 50.7%, 0.98, 218<br>989A3872.CR3: 50.2%, 0.97, 201<br>989A3839.CR3: 36.8%, 0.81, 186 | **good** | Clarity on the people only. |
| I4 | Balance group faces | works | 989A3870.CR3: 1.6%, 1.00, 143<br>989A3872.CR3: 1.7%, 1.00, 144<br>989A3839.CR3: 2.1%, 1.00, 143 | **good** | Lifts darker faces toward the group's median face; nobody is darkened. |
| I5 | Match group skin colour | works | 989A3870.CR3: 1.2%, 1.00, 143<br>989A3872.CR3: 1.1%, 1.00, 144<br>989A3839.CR3: 1.9%, 0.99, 146 | **good** | Moves each face a little toward the group's median colour, each person's own colour kept. |
| I6 | Relight | works | 989A3870.CR3: 4.4%, 0.92, 164<br>989A3872.CR3: 4.2%, 0.88, 191<br>989A3839.CR3: 1.3%, 0.67, 182 | **good** | Directional light across the people from the chosen angle. |
| I7 | Soften harsh shadows | works | 989A2698.CR3: 1.2%, 1.00, 717<br>989A2699.CR3: 1.2%, 1.00, 707<br>989A2703.CR3: 1.1%, 1.00, 713 | **good** | Opens hard shadows on faces in sun. |
| I8 | Backlight rescue | works | 989A2925.CR3: 42.7%, 0.77, 724<br>989A2699.CR3: 46.9%, 0.65, 736<br>989A3272.CR3: 0.0%, 1.00, 834 | **good** | Lifts backlit people against a bright backdrop (2925, 2699) and does nothing when they are not backlit (night shots, 3272). |
| I9 | Face-aware vignette | works | 989A2698.CR3: 63.0%, -, 170<br>989A2699.CR3: 63.4%, -, 168<br>989A2703.CR3: 60.9%, -, 180 | **good** | Vignette that darkens away from the faces, never across them. |
| I10 | Face brightness | works | 989A2698.CR3: 4.8%, 1.00, 150<br>989A2699.CR3: 4.6%, 1.00, 129<br>989A2703.CR3: 4.0%, 1.00, 141 | **good** | Brightens or darkens the faces alone, feathered into the neck. |

## J workflow

| # | Tool | Window | Engine (photo: changed, inside, ms) | Grade | What was seen |
|---|---|---|---|---|---|
| J1 | ID photo 35x45 on white | works | 989A2698.CR3: 100.0%, -, 377<br>989A2699.CR3: 100.0%, -, 370<br>989A2703.CR3: 100.0%, -, 379 | **good** | Head measured crown to chin, levelled, on white using the learned person outline frozen into the recipe; clean veil and hair edges. |
| J2 | Auto crop: headshot 4:5 | works | 989A2698.CR3: 100.0%, -, 84<br>989A2699.CR3: 100.0%, -, 70<br>989A2703.CR3: 100.0%, -, 66 | **good** | Measured crops (headshot, half body, full body, square, story) around the main face. |
| J3 | Level the eyes | works | 989A2698.CR3: 100.0%, -, 97<br>989A2699.CR3: 100.0%, -, 89<br>989A2703.CR3: 100.0%, -, 116 | **good** | Levels the eye line and crops just enough to leave no empty corners. |
| J4 | Transparent PNG cut-out | works | 989A2698.CR3: 1 files, 4480x6720; The cut-out follows AURA's learned subject selection.; 989A2698-cutout.png | **good** | Transparent PNG from the learned subject selection: 48.6% transparent, clean edges. |
| J5 | Instagram grid | works | 989A2698.CR3: 6 files, 1493x1493; Post the files in the order of their post number: post01 first.; 989A2698-grid-post06-r1c1.jpg, 989A2698-grid-post05-r1c2.jpg, 989A2698-grid | **good** | Square tiles numbered in posting order. |
| J6 | Carousel slides | works | 989A3870.CR3: 3 files, 2240x2800; ; 989A3870-carousel-1of3.jpg, 989A3870-carousel-2of3.jpg, 989A3870-carousel-3of3.jpg | **good** | 4:5 slides that join seamlessly. |
| J7 | Before and after | works | 989A2698.CR3: 1 files, 8982x6720; ; 989A2698-before-after.jpg | **good** | Original and edit side by side at full size. |
| J8 | Enlarge 2x | works | 989A2698.CR3: 1 files, 8944x13416; Enlarged 1.996x rather than 2x, to stay under 120 megapixels.; 989A2698-x2.0.jpg | **good** | 2x Lanczos enlargement with no ringing at edges. |
| J9 | Face picker | works | 989A3870.CR3: 0.9%, -, 166<br>989A3872.CR3: 0.9%, -, 168<br>989A3839.CR3: 4.2%, -, 165 | **good** | Lists every face; choosing one limits face, skin and makeup tools to it, verified in the window. |
| J10 | Faces showing no white of the eye | works | collection: checked 27, flagged 16: 989A2704.CR3 (1 of 1 faces showing no white of the eye), 989A2705.CR3 (1 of 1 faces showing no white of the eye), 989A2706.CR3 (1 of 1 f | **weak** | A list of faces showing no white of the eye, not a blink detector: 16 of 27 flagged, including squints, glasses glare and veiled faces, while two brides with closed, shimmer-lidded eyes were missed. A verdict needs a trained eye-state model. |
