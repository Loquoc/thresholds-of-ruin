# Procedural 3D art: implementation and continuation plan

This plan records the current procedural-art foundation, its review limits and
the next useful work. The [art direction](../art/3d-art-direction.md) owns the
visual principles; the [3D client guide](../../crates/client-3d/README.md) owns
launch and diagnostic commands. This remains an independently verified frontend
prototype, not a completed art pipeline or a new gameplay milestone.

## Intended outcome

New creature recipes should compose body plans, correlated dimensions, materials,
garments, attachments and compatible motion. Improve shared construction rules
before polishing individual ornaments or isolated frames. Preserve compact Bold
Storybook proportions, expressive color, bright foreground lighting, adult
character silhouettes, a broad shallow male chest and open ceilings. Follow the
[Bold Storybook definition](../art/3d-art-direction.md#proportions-bold-storybook):
large anatomical heads, substantial hands/feet and shorter lengths establish
compactness; female figures remain lean and lightly athletic with moderate hips,
tapered thighs and clear knees. Adult women, their clothes and accessories should
be alluring while preserving balanced anatomy and species identity.
The [updated cast sheets](../art/rogue-concepts/README.md) guide species and
costume choices; their approved humanoid reference anchors the body review.

The existing implementation is mostly Rust recipes. Shared fitting data makes
future equipment composition more practical, but there is no general external
mob-authoring interface or interchangeable worn-armor renderer yet. The client
must continue to render only appearance and equipment disclosed by the server.

## What is implemented

### Sharing checkpoint — 2026-10-21

The `feat/rogue-creature-models` branch collects the current full-cast refinement
for review. Shared body dimensions, garment-owned cuts and ornaments, constructed
boots, layered armor, fitted accessories and connected lined cloaks improve
composition across native and replacement outfits. Zombie damage and Phantom
transparency survive outfit replacement. Small reusable leaf, feather, membrane,
beak, claw, shell and hoof components support further creature recipes.

The broader shape pass covers the winged beasts and birds, Aquator, Xeroc,
Flytrap, serpents and equines. Rattlesnake's extended resting pose supersedes
the coiled pose in its reference sheet. Ice Monster refinement stays deferred.
The dated reference index retains the supplied full cast and approved humanoid
anchor, with filenames, sizes and hashes checked against the originals.

This is a reviewable modeling checkpoint. Adult face/neck transitions, hair and
hood overlap, general moving-cloth collision, whole-cast motion review and
controlled crowd-cost measurements remain open. The following construction
summary and continuation order distinguish these limits from implemented work.

| Foundation | Implementation | Current boundary |
| --- | --- | --- |
| Body dimensions | `creatures/anatomy.rs` resolves biped dimensions, joint landmarks, limb/foot shapes and Storybook mesh scales once per rig | Guardian and Yeti retain distinct recipes; equines and centaurs retain their own dimensions |
| Build variation | Existing slender/average/sturdy choices link shoulders, waist, pelvis and muscle fullness | Average proportions and vertical bone lengths are anchors; this is not unrestricted morphology |
| Shared fitting | Geometry, robe clearance and animation support use the resolved body; lateral joint roots and garment widths follow it | Arbitrary equipment fitting and compatibility are not implemented |
| Torso layers | Cloth, leather and plate use material-specific rib/back ease; structured fronts bridge the chest with a fitted waist | Existing family cuts remain; sleeves and complete garment patterns need more work |
| Figures | Each woman rolls her own figure, leaning toward her kind's typical one; the chest form is sculpted on a parameterized chest wall (bipeds and the centaur rider); garments name their support | Shapes are tuned by eye; no scaled or furred woman yet exercises a flatter species chest |
| Necklines and openings | Garments own split, deep and court plunge cuts resolved against the wearer; outer shells and a woman's low-cut plate open along the same cut; armholes close under the arm; bare-legged garments are cut high | Edges are painted plus fitted bindings, so very coarse meshes can still show a stepped paint edge beside the binding |
| Soft tissue | Soft regions are skinned to spring-driven bones that answer the body's acceleration in any motion; plate holds them, structured garments damp them | One spring per side; no cloth-on-skin interaction or per-move tuning |
| Pelvis | One sculpted pelvis inside the fitted envelope; skirts measure it at the belt line and widen only where the thighs press | The envelope radii still bound fits elsewhere |
| Faces | Joined lower-face/chin forms, surface-fitted mouth contours and family-specific nose/fang/tusk clearance | Expression comes from sculpted geometry; no facial-animation system |
| Cloth | Each caster owns a continuous double-sided robe and bound sash, fitted to the final supported pose | Approximate body proxies and bounded contact correction cannot guarantee every contact is clear |
| Motion | Grounded support/swing paths, turning, interrupted actions, supported falls and family-specific locomotion | Still-image review does not establish real-speed timing or absence of foot sliding |
| Model resources | Live meshes remain referenced; idle reuse is bounded by payload and key budgets | The idle budget is not a bound on total CPU memory, live assets or VRAM |
| Shape and rendering | Connected organic joints, fitted trim, feather/floater forms, foreground lighting and material separation | Some forms still read as rounded primitives or a coating over anatomy |
| Non-humanoid composition | Explicit support/grasp limb roles, authored bend landmarks, profiled limbs and claw palms; oriented thick scutes and fang arcs | Species proportions and placement remain Rust recipes; there is no external monster format |
| Adult eyes and brows | Substantial fitted female brows and controlled arches; one almond eye boundary shared by clipped iris/pupil surfaces, fitted lids and a visible upper lash line; iris-only glow and Medusa slit pupils | Lower-lid rims, species expression and adult cheek/jaw planes still need style work |
| Review tools | Offline lineups (with `--women`, `--jump`, `--turn` and timed motion frames), fallen lineups, travel studies, appearance controls, camera azimuth and capture warmup | These are diagnostic views, not gameplay authority or automatic visual-quality scores |

The renderer retains remote-main's synchronous material-pipeline setup and CPU
DX12 frame pacing alongside adjacent shader-compiler discovery. Explicit renderer
and compiler overrides remain available. Live play and diagnostic views share
the camera shading setup.

## What is working, and what that evidence establishes

Before the current remote-main rebase, the shared body/profile pass was reviewed
across all 18 families and five individuals per family in ordinary and side views.
Sampled travel, actions and fallen poses preserved broad support and family
identity. Adult figure relationships, the male chest correction and accepted
human/humanoid mouth work survived that pass. These findings establish a useful
visual baseline; they do not automatically qualify every later source revision.

The first torso-material preview had a narrower review: Warrior, Sentinel and
Vampire variants retained their adult silhouettes and fitted waists. The clearest
gain was the Sentinel's continuous chest shell. Leather and cloth changes were
subtler and did not resolve the overall garment-construction problem.

The rebased PR snapshot was captured again in normal and side lineups. Ordinary
views were reviewed for Warrior, Mage, Sentinel, Scout, Hobgoblin and Vampire,
with Guardian, Yeti, Quagga and Centaur controls; the six torso families were also
reviewed from the side. No broad static proportion or outfit-fit regression was
identified. This extends the torso review, but does not approve real-speed motion
or every cloth contact. The following image is actual rendering, not a generated
target:

![Current procedural humanoid lineups](../art/procedural-humanoids-current.jpg)

Focused PR checks exercise cache eviction without losing visible geometry,
correlated dimensions and rig landmarks, closed finite neutral robe geometry,
structured torso surfaces and a launched native renderer. Existing appearance
identity, disclosed combat reaction and native attack witnesses own their
unchanged boundaries. Full supported-platform/profile coverage belongs to the
independent 3D CI matrix; focused local checks are not a substitute for that matrix.

## Current challenges

The updated Bold Storybook definition requires a fresh proportion and costume
review. Earlier construction checks and accepted snapshots do not prove that
anatomical skull size, compact lengths, adult faces, lean female anatomy or
alluring costume design meet this target.

1. **Garment construction remains the largest modeling gap.** Torso layers now
   have a little material-dependent room, but many sleeves and costumes still
   inherit the body's rounded form. Cloth needs supported regions, elbow and
   underarm ease, free edges and coherent folds; leather needs panel structure;
   plate needs protective shells and deliberate articulation/overlap.
2. **Body, outfit and motion recipes remain partly coupled.** `Style` still
   combines species, outfit and behavior choices. The resolved body is a useful
   seam, not complete separation of these concerns.
3. **Continuous cloth has bounded, approximate contacts.** A small intermittent
   angular dark patch remains documented from earlier travel review. Its exact
   surface identity was unresolved. Keep it behind broad construction work unless
   it exposes a general failure; repeated tiny crop/fix loops are not the priority.
4. **Real-speed motion is not approved.** Review foot plants, weight transfer,
   turn/stop/restart transitions, action interruptions and revival in motion.
   Do not infer timing quality from a contact sheet.
5. **Resource and update cost need controlled measurements.** Existing runtime
   observations are not isolated cloth, mesh-upload or GPU measurements. Preserve
   live references and bounded idle reuse while measuring representative crowds.
6. **References are aspirational.** The generated humans are taller than the
   selected compact in-game direction. The armor study exaggerates chest shaping;
   use coherent protective shells rather than copying separate chest cups.

## Continue in this order

### 1. Establish the exact reviewed baseline

Reassess representative anatomy and outfits against the updated Bold Storybook
definition before treating older proportion anchors as accepted. Compare the
anatomical skull, ribcage, pelvis, hands and feet separately from costume mass;
review front, side, rear and action silhouettes at gameplay and portrait scales.
Keep adult female appeal explicit while correcting inflated hip/thigh masses
through shared body dimensions and garment fitting.

Build the rebased frontend with its matching backend. Use fixed seeds, camera,
scale and lighting for comparisons. Finish the latest torso-layer review in
ordinary and side views, with a few travel/action poses and live gameplay.
Record the source revision and distinguish visual acceptance from a successful
capture or test. Extend the existing diagnostic renderer with the
[scripted scene fixture](#scripted-scene-fixture-design) described below; keep
existing lineup commands and reuse their composition and rendering paths.

### 2. Give garments their own reusable shape rules

Extend the current torso-material seam into supported/eased/free regions. Start
with Hero, Scout, Caster, Vampire and Sentinel recipes so one rule improves
several roles. Keep the anatomical support surface separate from the garment's
visible surface, and seat trim and straps on the final garment. Preserve fitted
adult curves and neckline identity; a continuous front does not require a flat
torso or a loose barrel. Give sleeves their own taper and articulation clearance.

Acceptance: the layer reads as cloth, leather or plate at gameplay distance;
waist/shoulder/hip relationships survive all supported builds; arm movement does
not expose broad disconnected surfaces. Review both sexes and all three builds
collectively, with unchanged families as controls.

Progress: limbs now have the first reusable cloth rule. A sculpt can be
inflated, so a sleeve or trouser leg is the anatomy beneath it grown by an
ease (filling the muscle hollows), with soft rolls gathered at the elbow and
below the knee; cuffs and bracers sit over that ease. Every clothed person
uses it, the Rogue folk included, and contract tests require the cloth to
clear the limb all round for every build. Draped skirts, tunic hems, coat
tails and the caster robe now hang free: their folds deepen toward the edge
and the hem drops where each fold swings out, while short edges such as cowls
stay neat. Cloaks, capes, tunic skirts, coat tails and rags now hang on
their own drape pivots: a cloak flares back as its wearer travels and trails
through turns, skirts sway with the stride, and cloth at rest barely stirs.
Cloaks now use one connected exterior and lining, sealed around a thick perimeter,
with clean or connected torn hems. Their gathered top fits the resolved upper back;
body build scales the shoulder and hem widths. The fitted upper drape reserves
selected rear strap layers so a baldric stays underneath it. Each wearer owns its cloth mesh,
while materials remain shared, including Phantom transparency. Two pinned rows
hold the upper attachment as the existing eased swing blends toward the hem.
Skirts and hair still follow their existing drape pivots. This is procedural drape,
not cloth simulation; general moving-body and floor collision remain unchecked.
Sentinel plate now articulates: overlapping lames form the
rerebrace under each pauldron and the cuisses over the thigh, each band tucked
inside the one above so bending cannot open a gap. Torso garments now have
all three regions: supported at the shoulders, eased over the ribs, and free
below the belt, where leather jerkins and vests hang as a short swinging
peplum over the tunic skirt. Coats and robes keep their own skirts.

### 3. Separate composition concerns without a new content format

Keep body morphology, outfit selection, material roles and motion constraints
explicit in internal recipe data. Use shared surface/attachment coordinates for
belts, shoulder layers, held items and future worn equipment. Preserve deterministic
appearance identity and include every future independent visual choice in cache
identity. Do not infer anatomy or equipment slots from display prose.

Progress: outfit selection is now internal recipe data apart from the body. An
outfit names what each slot wears: arms (bare, sleeved or plate), legs (fitted,
trousered or plate), boots (height, heel, buckles, cuff), hands, cuffs, an outer
layer (a jerkin, vest or coat, or a cuirass with tabard and gorget), the tunic
skirt and a cloak. Neckline, lower-panel split, hem trim and cutaway construction
belong to the outfit, including the outer jerkin's free peplum. Its split,
pigment and hem trim travel with the garment, independently of the wearer's
native costume. Open feminine bodices retain their shaped neckline on
sentinels, undead and other wearers. Plate coverage depends on the recipe;
the independent female breastplate now has an anatomy-resolved front opening.
Skirts are cut for the resolved pelvis and thighs, preserving fitted waists,
balanced hip contours and enough clearance for the wearer's legs. A shared cloth surface
also fits the jerkin's peplum over the actual tunic cut, including its placement,
Storybook scale, fold phase, raised trim and shell thickness. The outer waist
now attaches to the selected torso shell's actual waist section, including its
Storybook scale and front/back centre. Shell emission and seam fitting share
one resolved sculpt with the same garment cuts. The hip and hem still expand
around the lower layer. An emitted-triangle regression checks the attachment
around front, sides and back across native and Ranger wearers; the independent
lower-layer regression checks clearance around all four sides across
native and Ranger outfits, both sexes and all three builds. It also bounds
excess stand-off: a clear shell can still form an oversized shelf. The free
hip/hem dimensions are authored independently from the narrow sewn waist, so
fitting the lower layer does not inflate an already adequate hem. Clearance
is resolved on the cloth's radial vertex grid at matching physical heights and
angles. A lifted split edge expands its supporting panels without widening the
entire garment. Closed front edges remain coincident under asymmetric support;
open split edges keep their separation. This establishes resting clearance;
independently moving drapes still need contact review.

Baldrics are reusable accessories: the warrior, scout, ranger and knight use
one assembly fitted over their torso layers. Independently clipped outer
sculpts combine with the tunic beneath them; a short vest does not cut away
the shoulder support. The cuirass resolves once for rendering and attachment,
and accessory clearance includes raised heraldry. A grown support envelope is
traced before emitting the strap, avoiding normal-offset folds at tight waists.
Front and back strips share seam rows with shoulder and waist turns, forming
one closed loop with inner surfaces and real thickness. The shoulder and belt
landmarks set its span; buckle placement follows a height along the path.
Open buckle frames and tongues follow the strap direction and surface normal.

Shared shoulder armor now uses a broad faceted crown and a thinner overlapping
lower plate. Inner surfaces fit the resolved arm, including muscle fullness
and the clavicle insertion; turned edges expose the metal thickness. This
replaces the two rounded shoulder pads without changing the arm joints.

Harnesses, stocks and buttons, leaf bodices, neck bands and discs, and knotted
sashes are explicit outfit ornament recipes. Their materials and optional
shoulder straps or scarf travel with the selected ornament; the renderer no
longer looks up those costumes from the wearer's native kind. Mesh witnesses
transfer each recipe onto an unrelated wearer and remove it again. Neckline
and armhole edging, collar binding and armor gorget pigment also belong to the
outfit. Native and Courtier retain their authored trim; Commoner and Ranger use
their own cloth binding, and Knight uses its armor gilt. Ranger selects its own hip pouch
on every wearer; Native keeps the warrior's and scout's pouches.

Torso attachments trace the combined body and selected structural garment, so
an opening or short shell does not remove their underlying support. Sewn edges,
harnesses and scarf tails retain their authored pattern-plane coordinates while
their depth fits a grown support surface. Complete rows end where their full
width loses support, instead of clamping individual vertices toward the neck or
waist.
The native female vampire keeps her open bodice without the male cravat; both
retain their rows of gold buttons. Emitted-mesh witnesses cover native Medusa
bindings, Orc harness/scarf, transferred Orc layers and the sex-specific cravat.
Binding shading, neckline construction, painted edges and action-pose clearance
remain unfinished. A detailed ribbon-shading experiment is shelved while the
rougher Rogue creature families receive anatomy and silhouette refinement.
It has not earned visual acceptance and is not part of the current renderer.
Oblique visibility alone cannot prove
self-intersection: legitimate cap exposure can lie ahead of a more distant part
of the same curved strip. Static checks do not establish full costume approval.

Other native costume trimmings (lacing, cowls and loincloths) remain
recipe-specific. Body traits, including the medusa's serpents, and the zombie's
decayed, torn-shirt treatment survive wardrobe changes. Native leprechaun coats
retain their front cutaway. These construction rules preserve creature identity
while an exchanged garment carries its own design. Static mesh/topology and
clearance checks do not establish full motion/contact or visual acceptance.

The offline wardrobe dresses 18 humanoid model identifiers, including Mage and
RedMage; stone guardians and yetis retain specialized body recipes. Casters
exchange their native robes, sash knots and sleeve bells for the selected outfit
and restore them with Native, while retaining body identity and casting poses. Each dressed
look has its own cache identity. The bestiary's `--outfit NAME` dresses its
humanoids, and O, or `--wardrobe-cycle SECONDS`, redresses them mid-stride,
replacing only the model while motion carries on. Repeated recipe-level outfit
changes retain the individual's original cache identity: selecting an outfit
again reuses its model, and Native restores the original costume. Unchanged
head and hair assets are reused across sequential replacements. Offline selections
remain presentation for review; live creatures use their disclosed visual loadout
and cannot inherit offline overrides.

The [resolved head component](../../crates/client-3d/src/creatures/geometry/head.rs)
owns its headwear, neck adornments, earrings and neck-coverage dependencies.
One resolved value drives both mesh construction and cache identity. Unrelated
garment changes can still reuse the head, while a changed choker or standing
collar receives its own geometry. Fresh-versus-cached geometry tests exercise
both outfit orders on Human, Nymph and Phantom. Native close-ups compare the
same explicitly refreshed pose in both orders; a separate positive witness
requires the neck ornament to be visible in the selected pixel region.

Outfits also carry headwear: bare, a hood up or pushed back, or a closed helm.
A hood or helm covers hair and what is worn in it, and pointed ears fold inside
it; what the body grows, such as serpents, still shows. Redressing reuses the
head and hanging hair when the outfit leaves them alone.

Contact and hair fitting share a retained-source accessor. Initial fitting
keeps source coordinates available after render extraction; warm fitting reads
without mutable asset access. An application asset-event regression observes a
real mutation first, then requires zero modifications to existing meshes during
warm composition while private deforming meshes are still created. Existing
hair isolation and posed-contact tests retain their geometry and motion coverage.
This removes unnecessary invalidation; it is not a measured timing improvement.

Yeti torso, hips and limbs compose the shared `fur_parts::Coat` surface
component rather than rings of scattered tufts. Each patch supplies its body
ellipsoid, comb direction, clump length, color and seed. Staggered clumps
overlap continuous underfur; row heights vary to break up bands, and their crests and tips project along the body
curve using destination normals. The same closed lock emitter serves garment
caps and cuffs. Limb coats resolve their spans from the same elbow, wrist,
knee and ankle attachments as the rig, with small overlaps at either end.
Density has a bounded row/column count, and each complete Yeti
build is covered by a 35,000-vertex / 42,000-triangle interim ceiling and zero additional warm-spawn meshes.
The local debug models currently contain roughly 31,000–33,000 vertices and 39,000–41,000 triangles across
both sexes and three builds. The coat supplies its own continuous backing; furred limbs do not also build
covered skin or tube cores. Bent-joint witnesses cover the backing after removal.
These ceilings are regression guards, not accepted crowd-rendering targets.
The performance target is **100 or more visible creatures at 60 FPS**
(16.7 ms/frame). The separate
`crowd-hundred` live scenario repeats the existing crowd palette with 100 NPCs
and the player; distance 34 frames its spaced layout within Manhattan sight
range. A native functional run retained 101 body-bearing actors in the camera
throughout all 90 measured frames; the smaller crowd at distance 16 retained
25 throughout 176 frames. These runs captured screenshots to inspect placement
and are not performance measurements. The additive `frustum_creatures` report
tracks extrema over every timed frame after culling, excluding held props,
decorations and oversized hair/cloth bounds; any unavailable count makes the
extrema null. The original actor counter remains an instantiated-actor count.
The old smaller fixture at default distance had only 15 bodies in the camera
despite reporting 25 instantiated actors. Camera-frustum presence alone does
not establish unoccluded pixels. Three subsequent release-client runs without
screenshots retained all 101 bodies throughout their measurement windows and
recorded matching physical viewports, but remain below the frame-rate target.
The dominant reported GPU passes include opaque rendering, depth and shadows;
shared torso/head meshes lead the source geometry inventory. Cache-miss source
counts do not equal per-visible-instance work. Further density work and measured
comparison remain required; offline CPU composition timings do not qualify
this rendering target.

Closed arm and leg surfaces now use radius-aware sampling, constrained projection
and bounded conforming refinement. Their independent one-millimeter physical
error and radial silhouette witnesses cover small/large capsules, gathered cloth,
resolved limbs across sex/build variants and shared arm recipes. Rounded gathers
and the sleeve underside use the same fields for generation and fitting. Actual
offset muscle bounds include smooth-union growth before caller padding.
The combined surface/evaluator checkpoint passed 469 methods (zero failures, one intentionally
ignored subprocess fixture exercised by its lifecycle owner), and all four
native composition methods passed. Matched native captures cover the same 26
monster appearances on both sides: 1,861,887 to 1,721,261 source vertices and
3,280,784 to 2,999,532 triangles, retaining 565 meshes. The final cast used one
symmetric retry and no fine-grid fallback. Reviewed Centaur, Hobgoblin, Nymph,
Vampire, Troll and Mage views retain
their silhouettes. These are fixed-cast source counts, not crowd-instance or
frame-time comparisons. Fresh live saves deliberately vary cosmetic identity
through their persistent private salt, even at the same world seed; compare
fixed appearances when attributing geometry changes. The exact benchmark Mage
also retained its reviewed silhouette without fallback, using 94,525 source
vertices and 148,136 triangles against 100,187 and 159,460 before this work.
Upper-arm recipes resolve narrower clavicle/elbow features on their initial
grid. The local field estimate uses a 0.75 mm allowance within independently
tested one-millimeter physical error; guards cover 96 arm cases at 20,000
triangles each and the benchmark Mage forearm at 12,000. Focused optimized
checks passed all 22 SDF and eleven surface methods. Scalar and analytic
value/gradient queries share primitive expressions and ordered composition;
projection normally obtains both values in one traversal. Rejected surfaces get
one symmetric-difference retry on the same grid. This preserves the exact Troll
upper arm at 4,720 source vertices instead of the rejected candidate's 75,118.
Its regression retains independent physical-distance/silhouette controls and a
12,000-triangle budget. A fresh 101-body source inventory found six symmetric
retries and zero fine fallbacks across 1,847 cache-miss parts: 7,321,211 source
vertices and 13,405,296 triangles, which are not rendered-instance totals.
Three fresh release runs after this correction at 1800 by 1125 pixels retained
101 camera-visible bodies in every measured frame (474/474/461 frames, zero
unavailable counts). Each run identified the NVIDIA GeForce RTX 2060 on DX12.
They recorded 47.50/47.43/46.18 FPS; median frame times were
20.95/21.08/21.19 ms, p95 23.78/23.52/24.24 ms, and maxima
30.18/25.67/210.44 ms. The third run's hitch remains unresolved. The requested
60 FPS remains unmet. These current operating
points are not a controlled before/after comparison or evidence that projection
improved frame time. Head and torso density remain the leading shared geometry
follow-up. The controlled model workload comparison (version 4) used three balanced rounds and
validated all six runs without failure on the same machine. Human and Nymph
cold/warm construction and mesh storage improved against main. The Mage memory
spike was removed, but its construction regression remains: cold p50/p95/max
384.0/406.7/406.7 ms on main versus 482.5/538.6/538.6 ms here (nine samples per
side); warm 5.609/6.158/6.158 versus 7.457/10.111/10.111 ms. Its pose median
improves from 2.410 to 2.297 ms, but p95 rises from 2.700 to 3.419 ms (270
samples per side). Its cold mesh bytes
fell from 22,360,392 to 18,683,520, summed over the three cold samples in each
round. The shared field-evaluation cost remains a separate optimization target;
lower source counts do not establish uniformly faster construction.

Live creature and found-corpse preparation now runs off the update thread,
including fitting, hair and the articulated rig. Egg-shaped stand-ins retain
movement and selection while work runs; each completed model replaces its egg
on the next update without waiting for the remaining queue or a new server view.
Obsolete results are discarded, current movement/death state is preserved, and
weapon attachment reconciles when a hand becomes ready. One job bounds background
contention and completed payloads; both source caches retain their idle bounds,
with worker asset bookkeeping collected through Bevy's maintenance systems.
Native debug/release client checks cover movement while loading, replacement and
101 finished camera bodies. Quitting during unfinished work also passes locally.
Frame timing waits for finished models before settling; this responsiveness work
does not establish the still-open 60 FPS target. Offline art reviews and the
controlled model workload retain synchronous construction.

Live startup presents the embedded concept artwork in a software window before
starting the world or graphics device. Its bar and information follow completed
startup milestones, then hand over to the prepared game scene while creature
models continue behind their eggs. The illustration and controls fit the display;
startup can be cancelled without leaving its owned world running. Software
milestone captures remain separate from gameplay GPU captures. This improves
startup presentation without changing the offline review timelines or qualifying
the frame-rate target. The [3D guide](../../crates/client-3d/README.md) owns its
current behavior and capture locations.

Shared tube paths now refine by a one-millimeter model-space target,
Bezier control-hull flatness and radius-aware tangent variation. Authored knots,
radii and explicit low-poly settings remain intact; paint spans stay short or
retain the old larger-segment spacing. Bounded subdivision work and authored
angular limits keep refinement finite. Tiny tubes avoid unnecessary side
doubling. Independent dense S-bend references, closed-surface tests and the full
3D library cover the shared change. Remaining head/hand density still needs review. These are geometry counts, not release timings.
`yeti-coat.json` supplies fixed three-angle rest, walk and heavy-action review.
The current body-coat finish is not visually accepted: its close surface pile
still reads as tiled scales. Next work is dimensional stand-off/loft with visible
free tips, retaining root overlap, curved flow, closed topology, resize and mesh
budget coverage. Head fur, hand/claw integration and further silhouette refinement
also remain open.

The [shared organic arm surfaces](../../crates/client-3d/src/creatures/geometry/arm.rs)
serve bipeds and centaur riders. Rider dimensions resolve width, muscle and
shoulder spacing from the build profile while retaining elbow and hand reach.
Deltoid caps belong to the animated arm; a compact root bridges the clavicle
without raising the shoulder crown into fitted armor. Mesh tests cover build
volume, grip placement and joint coverage through supported bends. The native
`centaur-builds.json` review covers both sexes and all builds at rest and walking.

The current people recipes place the head on a short neck and slope shoulders
into the arms. Female pelvic breadth and thigh muscle volume are now independent;
moderate hips place the leg roots, while the build profile shapes a leaner upper
leg and clear knee taper. Emitted-mesh cross-sections cover six humanoid families,
both sexes and all three builds, retaining calf recovery, fine ankles, pelvis
depth and the existing knee/ankle landmarks. Garments use the same revised body
support. These geometric contracts do not establish full style acceptance.
Organic female thighs now share four blended anatomical masses: upper
attachment, quadriceps, distal taper and knee. Width and depth change
independently along the limb. Longitudinal overlap joins the masses without
intermediate pinches, and the upper-shin cap fits the knee's narrower contour.
Build choices retain their muscle differences. The
sculpt, trouser and plate fitting, mesh bounds and posed robe contacts consume
that same description; the robe's analytical rays and contact proxies include
the accumulated blend allowance. Emitted front/side sections check contour
variation and bilateral agreement and the combined knee envelope without moving either joint. Native
front, side, rear and action views remain the artistic acceptance evidence;
these geometric checks alone do not establish the intended silhouette. The
other limb recipes and the separate stone/fur body constructions retain their
existing forms. Medial/lateral planes and the combined knee/shin silhouette
still need continued review against the approved body reference.
Continue reviewing the anatomical skull independently of hair, adult
brow/cheek/nose/jaw structure, connected torso masses and action silhouettes. Long loose hair is one sculpted
fall from the crown, and cloaks, skirts and hair ease into their swings.

Kestrel flight and tail feathers and the Griffin wing feathers share
one closed vane generator. Broad faces narrow progressively into a point, with
a slim central ridge and thickness proportional to width. Family palettes and
Kestrel barring remain recipe choices. Staggered rows of short coverts follow
both spread and folded wing planes, overlapping the longer secondaries. Their
bowed outlines and raised rows keep the short feathers readable as separate
shapes. Folded primary fans mirror their feather planes across the body. Broad
body/wing contact and motion review remain separate work; improved feather
construction does not establish those.

Griffin now has a larger anatomical cranium, cheek and brow beneath a raised
crest, a broad sculpted neck with surface-fitted feather groups, and shoulder
masses that continue into shorter bent forelegs. Wider talons retain planar
support, while the lion hindquarters keep distinct hocks and paws. Griffin and
Kestrel share closed ridged hooked mandibles; their cere and palette remain
recipe choices. Focused geometry and motion tests and native front/side/rear
lineups support this proportion pass. Feather/wing joins, full action/fall
surface clearance and overall finish remain open.

Bat and Dragon skin panels now share a closed tensioned membrane fan with
curved scalloped edges, shallow inter-finger camber, smooth surface normals and
physical perimeter thickness. Explicit outward facing preserves outer/inner
pigments on both mirrored wings. Aquator toe webs reuse the same skin at foot
scale; Bat's wing bones are more substantial. Focused closure, anchor and
mirroring tests and native front/side/rear views support this construction pass.
Complete finger/body contact through every wing pose and the wider creature
anatomy remain visual review work.

Dragon now uses shorter bent fore/hind support vectors, connected shoulder and
haunch masses and broader grounded four-toed claw feet. Its anatomical skull is
larger, and its neck is a continuous sculpt rather than a narrow tube. Shared
`ReptileHead` dimensions align Dragon/Jabberwock lower-jaw geometry and hinges
with the rendered skull scale; their lower-jaw profile is broader. Both recipes
reuse the claw-foot construction. A settled Dragon barrel contract measures its
actual surface against the floor. These contracts do not establish full limb,
wing, neck or jaw surface clearance through every action and fall.

Standard worn boots now use a constructed last: a shaped leather upper above a
layered welt/outsole, plus a tapered block heel for the outfit choices that call
for one. Their raised arch is real geometry; heel and forefoot share a support
plane while flat boots retain their planted midsole. Native and replacement
boots resolve from the same last, and the moving robe receives its conservative
contact envelope. Bare, stone and furry foot recipes remain separate. Shaft,
cuff and buckle construction and complete foot/garment motion appearance remain
continued refinement work.

### 4. Review motion families at real speed

The current broad Rogue pass targets Jabberwock, Aquator, Xeroc, Venus flytrap, Emu and the two snakes. Jabberwock's raised
chest, bent hind support and free grasping arms replace its former four-post
stance; its rig roles drive support selection independently of species names.
Xeroc's large articulated maw and actual overlapping armor masses replace its
small flat mouth and painted shell seams. Aquator's shoulder girdle, haunches,
cheeked skull and fitted eyes establish a stronger amphibian body. Aquator and
Xeroc use short splayed limb profiles and broad grounded pads; walking and local
action support both retain the limbs' lateral offsets. Shared profiled limbs
end in rounded flesh, and Xeroc retains purple shanks and pads with pale nails.
The flytrap has broad fleshy leaves, a substantial stalk and deep cupped lobes
that open to show the red interior and close on contact. Its opposing cilia
reuse the mouth-frame fang rows; rosette and stalk growths share a leaf blade.
Reusable components and the
[future-monster composition procedure](rogue-creature-models.md#composing-future-non-humanoid-monsters)
keep those construction improvements available to later mobs. Whole-form review
still owns visual quality, including shell attachment, open-jaw contact and
silhouette weight. Emu's compact support vectors and body lift are resolved
jointly; substantial hocks, shanks and three-toed soles support its feather mass.
The throat reaches into the breast and the skull itself is larger. Both snakes
have larger broad skulls, hinged lower jaws and fangs that fold as they close.
Their head, jaw, tongue and floor envelope share dimensions. Rattlesnake
default rest follows the maintainer's extended-body direction, with raised head
and rattle. Whole-motion body
and attachment clearance still need review; tests and snapshots do not establish
whole-cast style approval. Ice Monster refinement is deferred while the maintainer
reconsiders its sheet.

Quagga, Unicorn and Centaur now share compact equine support dimensions. The
standing height resolves from the upper link, shorter cannon and actual hoof
sole; fore knees and hind hocks keep different offsets. Ribcage and haunch mass
balance the shorter supports. Broad closed hoof wedges supply a sloped toe,
defined heel and planar sole. Horse and Unicorn nasal bridge, muzzle and lower
jaw share one sculpt. The existing short-leg stride rule keeps the smaller
reach from dragging planted feet through turns. General whole-motion surface
clearance remains visual work, beyond these shape/support contracts. Centaur's
settled side fall uses the same barrel width for body and human-arm support; an
emitted-mesh regression checks all three equine families and builds against the
floor.

Review people, heavy humanoids, birds, equines/centaurs, serpents and floaters
together. Check travel, sharp turns, stopping, action interruption, falls and
revival. Fix common support/weight-transfer rules before individual pose garnish.
Retain different family rhythms and the current disclosed ability-selection rules.

Progress: a real-speed walk review now runs as a test. It walks seventeen
families at a steady pace through the actual animation and support systems
and measures how far each planted foot slides between frames. Every planted
foot's median slide is zero; it found the shortened griffin and dragon legs
dragging through their stance (a horse's stride on shorter legs), now fixed by
scaling each beast's stride to its leg length. The same measurement covers a
steady turn and a stop-and-restart for people, birds, equines and beasts:
planted feet yield at most about 2 mm a frame on tight turns, as the solver
intends, and settle without skating. A third measurement follows a walk cut
by an action, a death and a revival: interruptions move no faster than the
action's own lunge, and revival, which used to snap a body upright in one
frame, now rises out of the death pose over most of a second.

### 5. Measure cost, then expand equipment integration separately

Measure first-use mesh generation, warm reuse and cloth/mesh-update costs with
controlled inputs. Keep raw timing claims separate from correctness tests.
Interchangeable worn armor needs disclosed visual recipes, anatomical attachment
mapping, coverage/layer ordering, clearance/deformation rules and loadout-aware
cache identity. The client side of that now exists for review (see 3): plate
lames are fitted to the wearer's own thigh and upper arm and stay nested, for
every build and figure, and sculpts evaluate only the steps that can shape
each block of their grid, exactly near the surface, which roughly halves
first-use mesh generation for a dressed person. NPC equipment cannot be invented when its observation has no
equipment list. Protocol/content-format changes require their own agreed scope.

The first offline composition slice keeps the wardrobe preset separate from an
independent visual loadout. Steel helms, leather boots and travel cloaks resolve
over the preset's head, feet and back pieces; covered earrings and replaced boot
trim are suppressed in the resolved result rather than deleted from the preset.
Selection order does not change cache identity, redressing retains equipment,
and clearing equipment restores the selected preset. The native mage robe still
owns its pose-aware deformation when accessories are equipped. Review these with
`--wear helm --wear boots --wear cloak`; live connections reject the override.
The next torso layer adds an independently selected steel breastplate, fitted
around the body and selected outer shell while retaining sleeves, skirts and
other compatible pieces. Covered bodice ornaments are suppressed in the resolved
outfit; straps and pendants use the combined exterior support. Cloaks reserve
that same plate surface and correct upper-back clearance after their eased
swing, including room for the lining. This remains bounded torso/back fitting,
not general cloth collision. The female front uses the shared anatomy-resolved
neckline cutter and contoured chest supports; the male front keeps its broad
plate. An underlying cuirass is resolved once for rendering and outer-item fitting.
Review armor explicitly with `--wear breastplate --wear cloak`; omit `--wear`
for the princess gown review.

These layers support the existing dressable humanoid fitting recipes. Arbitrary
anatomy compatibility, additional armor regions and general equipment collision
solving remain further composition stages.

The planned equipment system must support compatible creatures beyond the
humanoid wardrobe group. Visible items include armor, weapons, shields, helms,
gloves/gauntlets, shoes/boots, capes/cloaks/robes, amulets, rings and
shirts/undershirts. Current wardrobe presets are combinations of visual parts,
not the eventual equipment-slot boundary.

Keep body identity and anatomical attachment/grasp roles separate from native
costume. Compose independently equipped parts with explicit coverage, inner-to-outer
layer order, fitting and motion clearance: shirts beneath armor, cloaks over
armor, hands holding weapons or shields, and jewelry attached to compatible
anatomy. Finger/ring and grasp compatibility must be explicit rather than inferred
from a humanoid model whitelist. Cache identity must include the actual visible
loadout. These are future requirements, not implemented gameplay equipment
support or approval of a protocol/content-format change; only server-disclosed
items may be shown.

## Living refactor notes: anatomy, attachments and fitting

These notes record implementation findings and intended direction as of
2026-10-23. Update them alongside meaningful generation/composition changes:
record what actually changed, the source of truth, observed limitations and the
next architectural implication. Replace obsolete claims rather than accumulating
contradictory progress notes. Proposed interfaces below are design intentions,
not implemented APIs or approval for a new format or implementation milestone.

### Current implementation

- [Anatomy](../../crates/client-3d/src/creatures/anatomy.rs) resolves
  `HumanoidBody`: dimensions, joint-root positions, limb/foot shapes and optional
  bust anatomy. Bust data already includes useful landmarks such as the fold,
  upper slope and apex. This is shared fitting data, but not a complete named
  attachment/landmark registry for every creature.
- [Creature rigs and looks](../../crates/client-3d/src/creatures.rs) supply named
  joints (`J`), attachment hierarchy and wardrobe/loadout identity. A joint is
  not automatically a garment landmark: the torso joint does not describe the
  waist perimeter, and a hand joint does not fully specify a weapon grip.
- [Garment geometry](../../crates/client-3d/src/creatures/geometry.rs) combines
  body measurements, local recipe constants and fitted surfaces. `NecklineCut`
  ([cloth](../../crates/client-3d/src/creatures/geometry/cloth.rs)) derives
  openings from anatomy; [torso](../../crates/client-3d/src/creatures/geometry/fitting.rs)
  and [pelvis](../../crates/client-3d/src/creatures/geometry/torso.rs) queries
  locate actual surfaces.
  Other heights, offsets and trim paths still live inside individual generators.
  There is no universal request such as “resolve right-hand grip” that all
  species and equipment currently implement.
- [Equipment composition](../../crates/client-3d/src/creatures/outfit/equipment.rs)
  is an offline visual loadout layered over a wardrobe preset. Region overrides
  and suppression rules are explicit but currently item-specific. The
  [breastplate](../../crates/client-3d/src/creatures/geometry/armor_parts.rs)
  uses body/underlayer surfaces and the shared neckline cutter. An existing
  cuirass is resolved once for rendering and fitting another item over it.
  This is not yet a general layer dependency resolver.
- [Cloaks](../../crates/client-3d/src/creatures/cloak.rs) keep anchored shoulder
  rows and use the actual armor surface for bounded upper-back clearance during
  swing. [Robes](../../crates/client-3d/src/creatures/robe.rs) have their own
  pose-aware leg-clearance behavior. These are useful working constraints to
  preserve, not a unified cloth solver or proof of arbitrary-item collision.

### Intended shared description

Distinguish three things rather than treating all placement data as points:

| Concept | Responsibility | Example |
| --- | --- | --- |
| Anatomical landmark | A meaningful position or region resolved from morphology | Waist, neckline, bust apex, shoulder root |
| Attachment frame | Position, orientation and owning animated joint, with explicit coordinate space | Right-hand grip, shield grip, neck pendant, cloak shoulder |
| Fitting surface and motion constraint | Shape between landmarks, layer clearance and behavior while posed | Belt perimeter, armor over shirt, cloak over armor, robe around moving legs |

Each body recipe should provide a shared attachment and fitting description.
Items request semantic roles; the anatomy resolves where and whether those roles
exist. Resolve side and multiplicity explicitly: creatures may have several
hands, unusual feet, wings or no compatible neck. Missing compatibility must be
an explicit result, not a guessed humanoid fallback. Rings need compatible digits;
weapons and shields need grasp orientation as well as a hand position.

Keep body morphology separate from garment design. Shared landmarks do not mean
identical silhouettes: female/male cuts, neckline styles, coverage, ease and
material behavior remain recipe choices fitted to those landmarks. Selecting a
female body must not merely stretch the male garment. Conversely, an equipment
item must not silently change the selected body or replace unrelated outfit parts.

Resolve layers from inner to outer with declared coverage and ownership. An outer
item should fit the actual visible support beneath it, decide which covered
ornaments are suppressed, and expose its resulting surface to the next layer.
Use the same resolved data for mesh emission, trim, attachments and deformation;
independent approximate copies caused the cuirass fitting failure. Anchors alone
cannot prevent clipping between anchors or during animation.

### Open panel garments: first shared cut

The additional `SylvanDuelist` outfit uses `Straps::Panels { width }` rather than
an outfit-specific branch in the renderer. Its recipe is in
[sylvan.rs](../../crates/client-3d/src/creatures/outfit/sylvan.rs); the reusable
[panel cut](../../crates/client-3d/src/creatures/geometry/panel_parts.rs) resolves
two shoulder-to-waist strips from bust apex, lateral bust radius, shoulder
breadth and build. The strips converge toward the waist while exposing the
sternum, lateral bust and torso sides. Their coverage predicate, embroidery
clipping and fitted binding use the same center/half-width function.

This is still the existing fitted-bodice construction: cloth color is carried by
the torso surface, with raised binding and embroidery, not an independently
simulated fabric shell. Free lower panels, lower-arm cloth and the cloak use the
existing separate drape mechanisms. Shoulder/waist endpoint heights remain
recipe coordinates; they should become semantic landmarks in a later migration.
Do not describe this as the complete attachment registry or a cloth solver.

Tests distinguish covered bust apices from intentionally open center/side areas,
including multiple builds, volumes and species. Male wearers explicitly resolve
the separate Ranger recipe; both existing princess recipes remain on the rail.
The initial long cutaway used a lateral split larger than its fitted radius;
front sections reversed their angular order, leaving broken-looking cloth and
floating trim in the native capture. A regression checks angular ordering along
every sampled row, and this recipe now uses a smaller opening. This is a recipe
constraint, not a general solution for arbitrarily large split values. A future
angular opening parameterization should preserve panel orientation by construction.

`SleeveCut::Draped` supplies opaque lower-arm cloth while preserving the existing
sheer `Gauntlet` cut. `Waist::CrossedBelts` uses the shared fitted band generator
with opposite slopes and separate surface clearances, rather than reusing
jewelry cords. The same band function still serves straight sashes unchanged.
`Skirt::waistline` optionally selects a level seam: Sylvan uses the lower belt
line, while existing paired princess panels keep their raised waist. The initial
inherited high seam intersected the crossing belts in close-up review. This is
still an authored height, not a shared semantic waist-attachment service.

Verification for this addition: 48 shared garment-part tests, the two focused
Sylvan generation/coverage checks, five dressing checks and four redressing
checks passed. Release all-target Clippy, the release client build, repository
quick gate and documentation checks passed. The actual-process test rendered
human, mage and orc women in motion; reviewed captures confirm the corrected
continuous lower panels and lower waist seam. An initial harness setup attempt
timed out rebuilding unrelated workspace helpers; rerunning with the existing
release helper directory passed with unchanged source during capture. Full
workspace debug/release suites and native mouse tests were not run for this
checkpoint. These are local, uncommitted changes; no publication or launcher
deployment was performed.

The reference's hip weapon is not an invented gameplay loadout: general sheathed
weapon attachment and disclosed equipment remain separate from clothing fitting.

### Inspection finding: held items need two frames

The first attachment inventory found a concrete duplication to remove during
incremental migration. Both
[`scene::update_held`](../../crates/client-3d/src/scene.rs) and the
[bestiary wearer spawn](../../crates/client-3d/src/bestiary.rs) construct the
same sword rotation and hand-local translation independently. The live path
used a different forward offset for the short sword; the preview used the
long-sword offset. This duplication is now replaced by `props::held_transform`,
which both paths call. Sword placement aligns the authored handle midpoint to
the existing hand-local grasp convention instead of estimating forward offsets.

The [prop generator](../../crates/client-3d/src/props.rs) authors the sword lying
flat. Its handle shaft runs from `(-0.235, 0.03, 0)` to `(-0.4, 0.03, 0)` before
item scale; the short sword scales the entire model by `0.62`. Handle geometry
and handle-frame metadata now share their endpoints and scale. A regression
measuring the emitted handle found the old greatsword midpoint 0.0275 local units
behind the target grasp. The new mapping removes that discrepancy for both sword
sizes and preserves their 0.62 size ratio. Those coordinates describe the existing
asset, not a proposed universal grip position.

The humanoid hand is itself a child of the forearm, translated by `body.hand`
and scaled by `1.22`. Its finger geometry also depends on body build. Hand
animation is applied to that joint. Centaur hands have a separately authored
rest offset, and their native spear has its own attachment rotation. A blanket
replacement of every hand attachment would therefore change working poses or
misapply a humanoid fist assumption to other anatomy.

The intended relationship is **body grasp frame × inverse item handle frame**:
first locate/orient the grasp in the animated hand's local coordinates, then
align the item's authored handle to it. Define the scale policy explicitly;
parent scale must not accidentally resize equipment merely because a hand is
larger. A hand-local grasp and a prop-local handle need documented axes and
units, and neither is the hand joint origin by definition. Preserve item size
separately from matching the grip unless the item recipe explicitly permits
resizing.

Before changing these consumers, establish witnesses for the actual handle
axis passing through the grasp, blade direction, short/long weapon size,
nonuniform/parent scale, and attachment stability through wrist animation and
redressing. Check preview and live presentation against the same resolved
attachment. Keep the missing-creature/no-invented-hand contract. Determine which
non-humanoid grasp roles can supply a compatible frame before expanding support;
a joint called `HandR` is not sufficient evidence of a usable grip.

This is an item-side migration, not a completed anatomy attachment API. The
hand target remains the legacy local convention `(-0.03, -0.02, 0)`; it is not
yet resolved from each body's finger geometry. Existing parent-hand scaling and
non-sword presentation are preserved. Missing sword-handle metadata returns an
explicit absence, but the consumer still retains its old fallback for other
items. Replacing that fallback, resolving body grasp frames and defining equipment
scale independently of parent scale remain outstanding. Emitted-handle tests
cover alignment, blade direction, item size and wrist-frame composition; they
do not prove arbitrary hand fit or nonuniform scale compensation.
The live-scene integration witness covers equip, short/long weapon replacement,
unchanged-entity reuse and unequip cleanup. Native bestiary captures and a warrior
portrait show the held sword; five-person wardrobe lineups deliberately omit
weapons and cannot establish this rendering contract. A whole-bestiary wardrobe
cycling attempt timed out while repeatedly redressing 16 humanoids, with pending
capture files. Its cause is not established, and it is not passing stress evidence.
The fixed-outfit native case passed; broad cycling/readback responsiveness remains
an investigation item separate from handle alignment.

### Migration and evidence to preserve

Inventory embedded coordinates and their spaces before moving them. Introduce
named accessors around existing resolved anatomy first; migrate one shared
consumer at a time with before/after rendering evidence. Avoid moving constants
into a new file and calling that a semantic landmark system. Keep unresolved
questions explicit: surface ownership/lifetime, rest versus posed frames,
multiple attachment roles, layer conflict policy and cache invalidation.

Preserve deterministic appearance and loadout-aware cache keys, reversible
redressing, anatomy compatibility, feminine/masculine garment choices, and current
robe/cloak clearance. Validate across species, builds and poses using resolved
landmarks, emitted triangles and native captures. Numeric clearance checks do
not establish visual quality: the current breastplate neckline still has visible
edge faceting despite passing coverage checks. Keep gown-only captures separate
from equipment studies so optional armor cannot conceal the outfit under review.

Gameplay item disclosure, protocol changes and external content formats remain
separate decisions. The refactor should support the planned armor, weapons,
shields, headwear, handwear, footwear, outerwear, jewelry and underlayers without
claiming those slots or arbitrary creature compatibility are already implemented.

## Review and verification discipline

Keep the original dirty development checkout protected. Use the isolated art
branch and preserve unfinished work before rebasing. Never edit checked Rust
inputs during a build or run competing owners against the same target directory.

During model refinement, the maintainer requested simplified testing for rapid
iteration: run focused model/motion regressions and native previews, and set
aside mouse tests for these passes. For the 2026-10-21 sharing checkpoint, the
maintainer explicitly authorized pushing this feature branch without running
the push gate. This task-specific adjustment does not change the repository's
default gates or the complete Windows/Linux debug/release CI required before
merge.
Run the targeted art-contract tests, the renderer checks affected by rebase,
documentation/link checks and the native offline lineup witness. Live-backend
and combat process coverage remains with CI. Record failures
and limits honestly. The independent 3D workflow runs Windows/Linux debug/release
verification; its full result is required before merge. Do not weaken tests or
claim manual visual approval from a passing automated suite.

For the sharing checkpoint, `cargo test --manifest-path
crates/client-3d/Cargo.toml --workspace --lib --locked` passed 206 renderer
library tests and the model-routing library test. One subprocess fixture is
intentionally ignored for direct invocation and exercised by its owning
lifecycle test. The 18 documentation tests passed. Formatting and warning-free
Clippy passed on the same code. Both focused native process owners passed:
Rogue family lineups reached captured GPU frames, and the live wardrobe test
cycled replacement outfits across nine wearer families. The latest native
binary and its recorded source fingerprint match the shared code. Earlier
shape passes reviewed front, side, rear, action and settled-fall captures;
the latest footwear pass adds native and replacement-outfit views.
These results do not qualify the current tree in release or establish a full
verification-gate pass. Earlier broad renderer checks predate the final equine
and footwear changes; earlier native mouse failures remain recorded as
failures, not passes.

New body/outfit behavior should extend the narrow owning regression tests and
actual-process witness. Existing interaction, appearance identity and authority
tests remain relevant without duplicating their assertions in art-specific tests.

### Contract witnesses

| Contract | Owning witness | Limit |
| --- | --- | --- |
| Bounded idle reuse while visible meshes survive | [Cache regressions](../../crates/client-3d/src/props_art_tests.rs), including key and byte boundaries | Caller-owned insertions become idle when dropped and are accounted for by periodic pruning |
| Correlated bodies and actual rig landmarks | [Body contract tests](../../crates/client-3d/src/creatures/art_contract_tests.rs) | Does not grade anatomy or character appeal |
| Continuous structured torso and fitted waist | `geometry::art_contract_tests` in [geometry](../../crates/client-3d/src/creatures/geometry.rs) | Sampled surface properties, not full garment/contact approval |
| Native/swapped torso binding, harness/scarf support and sex-specific Vampire cravat | [Emitted garment contracts](../../crates/client-3d/src/creatures/geometry/garment_tests.rs) and native Rogue/wardrobe process owners | Static contact, coverage and recipe checks; binding shading, neckline construction, painted edges and motion clearance remain open |
| Connected lined cloaks, supported yokes, pinned motion and rear strap layering | [Cloak contracts](../../crates/client-3d/src/creatures/cloak_tests.rs), [owned sheet tests](../../crates/client-3d/src/creatures/cloak.rs) and emitted garment contracts | Native/replacement selection and mesh construction are checked; general body/floor collision and whole-motion appearance remain open |
| Finite closed neutral cloth and independent per-wearer deformation | `robe::art_contract_tests` in [robe](../../crates/client-3d/src/creatures/robe.rs) | Topology and mesh ownership are checked; full contact clearance and real-speed continuity remain manual review |
| Default backend, single-sample cameras and software frame pacing | `graphics::tests` in [renderer setup](../../crates/client-3d/src/graphics.rs) | Native process evidence supplies actual renderer startup/readback |
| Observer-independent disclosed appearance | `scene::tests::observer_role_and_interaction_target_do_not_change_the_rendered_appearance` in [scene tests](../../crates/client-3d/src/scene/tests.rs) | Existing identity boundary retained |
| Generated models presented by a real native window | `Native3DProcesses.test_art_lineup_presents_generated_bodies_and_accepts_native_close` in [process tests](../../crates/client-3d/scripts/test_client_process.py) | Screenshot integrity and native window lifecycle, not automated aesthetic judgment |
| Disclosed combat still dispatches real client motion | `Native3DProcesses.test_native_click_attacks_the_selected_actor_and_plays_its_motion` in [process tests](../../crates/client-3d/scripts/test_client_process.py) | Existing server/transport/native-input witness retained |


## Scripted scene fixture design

Status: proposed extension to the existing offline client, not implemented. The
user requested repeatable scripts for composition, placement, cameras, animation
triggers and screenshots. This is review infrastructure within the model-quality
work; it does not choose the game's future runtime scripting language or change
protocol, save or scenario formats. Keep existing lineups as shortcuts.

### Existing mechanisms and the actual gap

- [Bestiary](../../crates/client-3d/src/bestiary.rs) builds seeded bodies, applies
  wardrobes and equipment, schedules actions and moves the review camera. Its
  `lineup` uses `Time<Real>`, named shot deadlines and pending-readback checks.
  Other directors use the local `clock` function, including travel-study time.
- [Animation](../../crates/client-3d/src/creatures/animation.rs) advances `Anim`
  using a capped frame delta, derives walking from root displacement and retains
  contact state. Sampling only an action timer cannot reproduce the full pose.
  Cloth and smoothing retain history too; they must advance with the same clock.
- [Diagnostics](../../crates/client-3d/src/diagnostics.rs) already observes GPU
  screenshot completion, bounds in-flight readbacks and reports renderer and
  actor evidence. Reuse that mechanism rather than a separate image renderer.
- [Client setup](../../crates/client-3d/src/lib.rs) orders animation and related
  presentation systems. Explicit ordering through transform propagation and
  render extraction is needed before a scripted screenshot is requested.

A wall-clock wrapper around existing CLI switches cannot guarantee a selected
pose. The fixture must drive production composition and animation systems with
controlled steps, including garment motion, without creating another poser.

### Capture hold foundation

The offline bestiary now installs
[ReviewClockPlugin](../../crates/client-3d/src/review_clock.rs). Before Bevy updates
its time resources, a pending screenshot pauses virtual presentation time; the
clock resumes after the last pending request disappears. Real time keeps moving
for deadlines. The barrier releases only a pause it acquired, preserving a
preexisting pause. Animation, cloth and soft-body smoothing consume the common
virtual clock. The plugin is installed only in the offline bestiary, leaving
the live client's clock source unchanged. Animation now skips zero-duration
updates in both paths: recomputing a pose without advancing time could still
change contact-dependent joint transforms.

The regression uses a generated character performing a jump and checks that its
action phase and joint/cloth transforms remain unchanged across delayed readback
updates, then advance after completion. Previously only the lineup's camera and
family were held. This is a capture barrier, not yet fixed-step script execution:
script parsing, integer tick advancement, scene/event controls, readiness checks
and per-capture manifests remain to be implemented and verified.

The native acceptance test now checks hold/resume timestamps in the actual client
and validates the named rendered images. Unit coverage also preserves real-time
watchdog progression and a preexisting pause. The generated-body regression,
existing animation/bestiary checks and the wardrobe composition matrix pass for
this foundation. This evidence establishes the capture hold, not deterministic
fixed-tick replay or completeness of the proposed fixture runner.

### Script contract

Proposed entry point: `--scene-script PATH --capture OUTPUT`. This spelling is a
proposal, not a supported command. Use strict, versioned declarative TOML with
unknown fields rejected. The initial vocabulary should cover the following:

| Record | Required behavior |
| --- | --- |
| Scene | Schema identity, deterministic seed, fixed step rate, viewport dimensions, lighting, background and ground |
| Actor | Unique ID, model identity, explicit body variation/seed, wardrobe, equipment, world position, orientation and scale |
| Camera | Named perspective or orthographic camera, explicit position and target, up direction, clipping range, field of view or orthographic extent |
| Event | Integer tick, stable sequence order, actor/camera target and typed operation |
| Composition change | Redress or equip/unequip through existing composition APIs; preserve unrelated actor identity and item slots |
| Placement/movement | Explicit teleport versus motion segment; movement segments advance root position per tick so locomotion receives displacement |
| Animation | Typed existing action, with explicit start/restart or cancellation semantics; unsupported actions fail validation |
| Capture | Unique safe name, tick and camera; wait for the corresponding completed GPU readback |

Use integer ticks at a declared fixed rate as the authoritative timeline; display
seconds only as a derived value. Validate rates against animation's delta cap so
one requested step is not silently shortened. Tick zero is the initialized pose.
For subsequent ticks, apply ordered state-changing events, advance one step,
propagate transforms and render, then service captures. A zero-age action capture
at tick zero needs an explicit initialization path, not a fake movement step.
Multiple cameras at one tick render sequentially while model state stays frozen.
Reject nonfinite values, invalid projection ranges, coincident camera/target,
duplicate IDs or capture names, unknown references, conflicting modes and output
paths escaping the chosen run directory. Bound actors, events, duration and
readback timeout so malformed scripts cannot run indefinitely. Unsupported
anatomy/item combinations must fail explicitly or record an explicitly requested
fallback; silent Ranger substitution would conceal a fixture authoring error.

### Execution and capture barriers

Use a director state machine: validate, compose, prepare, advance, render/capture,
finish or fail. Real time is for watchdogs and rendering readiness only. Warmup
must not age animations, cloth, procedural lighting or camera motion. Readiness
should include scene creation and relevant render preparation; a fixed sleep is
not evidence that pipelines are ready. Put a bounded failure deadline around it.

All presentation consumers affecting a fixture must share the controlled clock:
actor/root motion, action timers, gait/contact state, smoothing, robe and cloak
motion, effects, procedural lighting and camera interpolation. Audit their time
sources before claiming deterministic capture. Preserve ordinary live-client time
behavior. Advance from a fresh initial state through every fixed step; arbitrary
seeking must reset and replay rather than overwrite timers on an evolved rig.

At a capture tick, hold state until transforms and render extraction represent
that tick. Do not advance while its readback is in flight. Record completion only
after a valid image has been written, using a temporary file and final rename.
Do not infer success from a placeholder file or a queued screenshot entity.
Finish successfully only when every declared capture has a matching completion
record; missing/failed captures produce a nonzero exit and an incomplete manifest.

### Evidence and acceptance

Each run gets a fresh output directory and manifest containing script bytes/hash,
resolved configuration and actor variants, source revision plus dirty-input hash,
executable identity, viewport and camera state, fixed-step rate, event order,
capture tick, resolved action/phase, renderer/driver details, image dimensions and
hashes. Distinguish the pose requested from the pose actually submitted for
capture. Keep routine output machine-local. A test runner supplies build identity;
the renderer must not fabricate provenance it cannot establish.

Repeatability means matching resolved state and tick progression on the same
inputs, with an explicitly scoped visual comparison. Do not promise bit-identical
pixels across GPU vendors or drivers. Pixel checks need controlled render settings
and tolerances; geometry, transforms and semantic state remain separate evidence.
A valid PNG alone never proves correct fit or animation.

Implementation acceptance requires:

1. Parser/validation tests for every supported record and rejection boundary,
   including invalid references, duplicate output names and path containment.
2. Director tests proving stable same-tick event order, delayed readback without
   pose advancement, timeout/error propagation and complete-capture accounting.
3. Composition integration tests for multiple bodies, wardrobes and visible gear,
   preserving identity and unaffected slots across scripted changes.
4. Timing integration tests for walking, actions and cloth, comparing state at
   capture ticks despite different wall-clock/render delays. Retain existing live
   animation and lineup behavior tests.
5. Actual-client process tests that load a fixture, render named front/side/back
   and moving views, validate their manifest/image agreement, repeat the run and
   exercise a failing script/capture. Use the client executable, not a mock.
6. Human visual review of the captures. Start with Sylvan bust/build variations,
   both mage bodies, a broader nonhuman body, and outfit-plus-equipment layers.
   Keep an ordinary body/outfit fixture as a control against regressions.

Implement the shared clock and capture barrier first, then wire strict scripts to
composition, placement, cameras and events. This sequencing is not a reduced
scope: all requested controls and actual-process evidence are needed before the
scripted fixture is described as implemented. Keep the general model-quality goal
open after this infrastructure lands.

## Bounded composition coverage

The expensive wardrobe cross-product is now split into cheap exhaustive recipe
checks and bounded generated-body checks. The former still visits all 682
model/body/outfit combinations in its existing domain to verify dressability,
native identity restoration, recipe resolution and distinct wardrobe cache keys.
It does not construct meshes.

The generated-body test uses 47 permanent witnesses plus 32 seeded Monte Carlo
cases. Permanent witnesses cover native and Knight outfits on each of the 11
model identities and both body variants, the female Sentinel's loose hair in
Courtier clothing, and Medusa's serpents in Commoner clothing on both variants.
Random cases vary model, outfit, individual seed, body build, bust volume, contour,
asymmetry and figure. Their fixed count does not grow with the outfit catalog.

Coverage ownership from the former exhaustive geometry test is explicit:

| Original assertion | Current owner |
| --- | --- |
| Dressability, native identity, distinct cache keys, recipe resolution | Cheap exhaustive recipe test |
| Helmet hides hair and contains pointed ears | Permanent Knight witnesses for every previously covered model/body pair |
| Female Sentinel shows loose hair outside her helm | Permanent female Sentinel/Courtier witness |
| Medusa retains intrinsic serpents | Permanent Native/Knight/Commoner witnesses and sampled compositions |
| Finite mesh vertices, hair state, cloak/skirt/peplum/layer counts, limb metal matching recipe | Same assertion helper on every permanent and sampled generated case |
| Ghost material exception | Preserved in the shared helper; permanent native/Knight ghost witnesses |

The expensive test no longer proves the entire cross-product. The dedicated bust,
hip, neckline, layered-skirt, mage-redressing, equipment-clearance and animation
regressions remain unchanged; random cases do not replace known failure witnesses.
The earlier exhaustive result remains historical evidence for its tested inputs,
not a claim about future combinations.

`TOR_COMPOSITION_SEED` selects a decimal unsigned 64-bit batch seed; the default
is stable. SplitMix64 is specified in the test helper to avoid changing replay
when a random-number library changes. Before each generated case the test emits
its batch seed, index and resolved model/outfit/body/individual seed into captured
test output, so a failure can be replayed with the same seed and source revision.
Successful output stays in local test logs. Invalid seed text fails explicitly.
The script fixture runner is not implemented yet, so these are replay records,
not executable scene-fixture files.

Future CI can rotate recorded seeds and weight extra samples toward changed
parts and known conflict classes. That rotation and weighting are not automated
by this first bounded sampler. Keep permanent regressions, budgeted random cases
and actual rendered/animation evidence distinct in reports. Promote a newly
understood random failure to a focused permanent regression rather than enlarging
the full Cartesian product.


### Optional fixed-step lineup clock

`--lineups --review-step-hz 50` selects a fixed presentation increment per
rendered update. Rates from 20 through 240 Hz are accepted; lower rates would
exceed the production animation delta cap. The review clock snapshots virtual
time before Bevy's wall-clock update and restores it with one selected step
afterward. The generic presentation clock receives that same value, so animation,
cloth and smoothing share the increment. Real time remains untouched for process
deadlines. Pending screenshots advance by zero and retain pause ownership
semantics. Existing variable-time lineups remain the default.

In this mode lineup deadlines also consume presentation deltas. The option is
restricted to offline lineups and rejected for live connections or galleries.
This makes frame progression repeatable; it does not establish deterministic
GPU pixels, shader readiness or a complete scripted timeline. The rate is rounded
to a nanosecond duration; authoritative integer script ticks, explicit events,
cameras, scene manifests and initialization remain pending. In particular,
existing lineup view deadlines may clamp their local schedule independently of
the animation step; they are not yet exact scripted capture ticks.


### Fitted waist attachment ownership

The fixed-step review exposed a mismatch between fitting and animation ownership:
ordinary belts followed the torso, but crossed leather bands and sashes were
emitted on the independent hip joint despite being fitted to the torso surface.
A generated-body jump regression reproduced loss of the crossed band's intended
clearance within two updates. Increasing mesh clearance would only mask that
frame mismatch.

`Waist::attachment()` now declares the owning joint for shared waist components:
sashes, crossed cords and crossed leather bands use the torso; jeweled skirt
frameworks and low coin ornaments use the hips. Generation transforms fitted
components from their authored hip coordinates into the torso attachment frame,
using the same path as the ordinary belt. Native caster accessories preserve
their existing coupled hip/robe frame. Geometry, colors and outfit recipes remain
unchanged.

This is an explicit component-level attachment rule, not the future complete
landmark registry. A later fitting description should carry the owning frame with
surface/anchor results so emitters cannot silently fit in one frame and animate
in another. Hanging tails may ultimately need separate secondary attachments;
this change keeps each current accessory rigidly with its declared parent. It
does not claim to eliminate all hip shelves, layered-cloth gaps or bust-panel
issues visible in the review.


### Paired panels resolve each side independently

The shared open-bodice panel fitter previously mirrored the right-side apex and
used the unscaled base bust radius for both halves. The anatomy already resolves
independent mass radii and apex heights for asymmetric bodies. A regression at
the resolved lateral panel edges showed the fuller left side losing its intended
coverage; the reverse asymmetry is covered by the same test.

Panel span resolution now accepts a side and uses that side's mass radius and
apex height. Coverage and the raised binding both call that resolver. Symmetric
bodies keep matching spans, while the shared shoulder and waist endpoints remain
unchanged. This is fitting behavior of the reusable panel component; it does not
replace the outfit or alter the authored neckline/coverage fraction. Tests cover
both asymmetry directions, symmetry preservation and binding centers staying on
the corresponding panel.

For the later fitting refactor, paired landmarks must remain side-specific from
anatomy through pattern coverage and decorative geometry. Mirroring a resolved
right-side fitting result silently discards asymmetry even when the underlying
body is correctly generated. Full panel shape refinement against the reference,
and a complete named landmark registry, remain separate unfinished work.


### Asymmetric fitting of centered necklines

Sweetheart/strapless top edges previously mirrored the right-side mass, while
plunge clearance used only the right apex and the unscaled base radius. Focused
regressions reproduced a left peak missing its resolved slope and a left-side
plunge exceeding its intended clearance on an asymmetric small-volume body.

Sweetheart peaks now use each side's resolved mass. Both halves share the lower
of their fitted center dips and a common level back, keeping the center
continuous rather than introducing a seam when the fit switches sides. Plunge
openings remain centered and symmetric, but their width is constrained at both
resolved apex heights using the corresponding mass radius. Coverage and raised
neckline binding already share these evaluators, so the corrected fit reaches
both paths without a second outline implementation.

The tests preserve center continuity and exercise both asymmetry directions,
including small and large volumes for plunge clearance. This extends the panel
fitting correction to shared sweetheart and V-cut geometry. It is not a claim
that every garment resolver is side-aware: armhole, halter, one-shoulder and
centaur-wrap paths still need their own fit/continuity review before a complete
landmark abstraction replaces the remaining local calculations.


Visual follow-up: the post-change royal-princess close-up shows small jagged
cloth-color patches beyond portions of the gold neckline binding. The numerical
edge/clearance regressions do not prove a clean rasterized boundary. Investigate
how mesh tessellation and vertex-color coverage meet the fitted ribbon; preserve
this distinction when assessing the mathematical fit versus rendered quality.


### Hard garment coverage boundaries in sculpted meshes

The close-up's cloth-color spill had a mesh-level cause: a surface-net triangle
could contain both skin and cloth vertex pigments. Raster interpolation then
blended the two across a fitted neckline even though the coverage evaluator and
binding path agreed mathematically. An emitted-mesh regression reproduces this
with a hard two-color cut on a sculpted sphere.

`Sculpt::mesh_partitioned` accepts an optional binary coverage evaluator. The
ordinary mesh path remains available. Crossing faces are locally subdivided and
clipped against that evaluator; the two regions share geometric intersection
positions but sample their pigments just inside their own side. Surface position,
winding, interpolated normals and baked occlusion are retained. Interior faces
keep their original cached attributes. Corner, midpoint and center samples catch
cuts turning through a face whose corners agree, without increasing the entire
sculpt grid's resolution. Body-painted bodices use this for their garment/skin
coverage, including neckline openings and bikini coverage.

This is a bounded piecewise-linear boundary approximation, not a replacement for
true separate cloth shells or per-pixel material masks. Features smaller than its
sampling scale can still be missed, and detail-pattern or wound-color boundaries
are not classified by this garment mask. The tests check mixed-pigment removal
in the final mesh, area and winding preservation, normals/shading, unchanged
interior handling and a region inside otherwise matching corners. The native
renderer and close-up review are still needed to assess the visible result.


The matching post-change princess close-up shows a cleaner central neckline,
with the stair-stepped cloth-color spill removed there. Cloth still rises above
the binding toward the underarm. That remaining front/back outline disagreement
needs a separate fitting investigation; successful hard-boundary mesh tests do
not establish that every binding follows the complete three-dimensional cut.


### Closed upper-edge binding on the fitted surface

After hard pigment boundaries were corrected, a separate underarm mismatch
remained: the binding used independent front/back projected paths, while the
bodice used a continuous front-to-back coverage blend. The emitted-binding
regression found a side vertex roughly 0.034 model units below its actual cut.

Sweetheart and strapless binding now trace a closed curve around the torso.
At each angular sample a bounded height solve finds the upper edge on the actual
sculpted support, evaluating the same coverage height and shared `bodice_front`
blend. A narrow band follows that surface with outward-normal clearance. This
replaces the disconnected front/back path assumption and its side extrapolation;
it does not change the authored garment cut. The new `neckline_parts` component
owns the surface tracing and emission, separate from outfit recipes.

Tests compare emitted trim with coverage, check that the resolved curve closes,
and check surface distance and edge-height agreement around Human, Mage and
Nymph bodies for both supported cuts. The tracer assumes a single upper edge
within its supplied height bracket and a radial torso surface. Arbitrary cutouts,
compound necklines and non-radial creature surfaces still need explicit tracing
contracts; this is not yet a universal attachment/landmark registry.


The first surface-traced implementation failed the whole-torso edge-agreement
test because the general-purpose sphere marcher's coarse hit tolerance made its
returned radial point jump during the outer height solve. This path now uses a
bracketed radial solve with tighter precision; the original edge-agreement
tolerance was retained and passes. The general sculpt marcher is unchanged.
The retained fixed-step princess close-up confirms the gold binding now continues
around the visible underarm curve; the previous large cloth-over-trim mismatch
is removed there. A narrow purple rim and faceting remain visible near parts of
the edge, so this is not a claim of pixel-perfect alignment between the sampled
paint boundary and offset trim. Garment regressions, sampled compositions and
the native princess motion check pass. The full scripted fixture and broader
surface/attachment contracts remain future work.

### Binding clearance and visible cut overlap

The closed trace initially solved the body edge and then lifted its trim along
the surface normal. That final lift changes height and lateral coordinates on
sloped surfaces, so an edge aligned before clearance can leave a visible cloth
rim afterwards. An emitted-strip regression checks that each patch overlaps
both sides of the coverage boundary for strapless and sweetheart cuts. It fails
on the previous implementation. The fitting solve now includes the normal lift,
and resolves the two edges at equal signed height margins about the cut. This
keeps the six-millimeter band centered on the visible boundary rather than
entirely below it. Rendered inspection and tests are required before claiming
that the remaining visual rim is resolved.

The matching fixed-step princess capture shows a more continuous front binding,
but a thin dark/purple line is still visible near the underarm. The clearance
regression proves overlap in the coverage field, not that the raised strip
perfectly follows the triangulated visible surface at every viewing angle.
Further geometry/occlusion work is deferred while the scripted review runner is
implemented. Garment tests, focused/seeded compositions and the native princess
motion process check pass for this change.

### Script runner implementation checkpoint (in progress)

The first executable runner uses strict JSON, reusing the client's JSON stack,
instead of adding the TOML dependency proposed above. The proposed broader
contract remains the destination. `--scene-script PATH --capture NEW_DIRECTORY`
is the entry point; other review/live options are rejected in this mode.
`crates/client-3d/review-scripts/wardrobe-motion.json` is a front/side/back and
motion example. These files are offline review inputs, not gameplay scripting
or a persistence-format commitment.

The initial schema supports exact model identities, explicit seeds and body
variations, wardrobe and the four currently composable equipment choices,
positions/yaw, perspective cameras, ordered actor events and tick/camera capture
requests. Events support action restart/cancel, velocity, teleport, dressing and
replacement of the explicit equipment loadout. Dressing preserves equipment;
equipment replacement preserves wardrobe. Teleport resets animation state and
retains velocity. Same-tick composition operations are applied in source order
and rebuild each actor once. All actors use the existing generator and rig.

Tick zero is the generated rest pose; action starts at tick zero are explicitly
rejected pending an initialization-without-advancement API. Later ticks apply
events, move roots, then advance the existing animation by one bounded step.
Camera switches and pending GPU readbacks hold the presentation clock. Output
requires a new directory; image writes use temporary PNGs and rename. A manifest
records the script, resolved composition keys, actor root/action state, camera,
tick, presentation time, renderer and image dimensions/fingerprints. FNV-1a
fingerprints are labeled as such and are not cryptographic provenance. Source
revision, dirty-input identity, executable digest, full joint/cloth evidence,
orthographic cameras, configurable scene lighting, and richer motion/attachment
operations remain unfinished. No claim of full fixture acceptance is made yet.

The first native process acceptance passes: it runs the same script in two fresh
processes, compares captured root/action and selected joint transforms, verifies
two cameras at tick zero hold identical actor state, and checks a same-tick dress
plus equipment change leaves no extra rig. A separate process test rejects bad
input before output creation. Clock/unit tests cover held phases, one-step
advancement, schema rejection and timeout failure. Render readiness waits for
tracked mesh uploads and an empty pipeline queue; all reported pipeline errors
currently fail the run, including potentially transient shader-loading errors.
That transient-versus-terminal classification still needs refinement.

The retained example captures show the duelist jumping and the native mage's
robe deforming as she walks. Initial actor positions were corrected to the fixed
stage ground at Y=-0.5 after visual inspection. The armored jump exposes small
skin-colored breaks near the lower sides of the fitted breastplate, plus the
already known waist discontinuities. These are concrete fitting regression
candidates; a successful script capture does not mean its models are free of
clipping. The example is now a reusable input for that investigation.

### Hair contact explains the apparent breastplate holes

The closer script capture and a hairstyle-only control correct the earlier
interpretation: the brown patches are long front hair locks emerging through
the armor, not skin escaping its fitted envelope. The added lateral torso test
passes both with all metal triangles and with front-facing triangles only.
Keeping the body, outfit, equipment and camera fixed while selecting a bun
removes the marks. A separate emitted-hair regression fails on the original
loose-hair mesh: one front vertex lies at Z=0.0074 while the plate at that XY
location is at Z=0.1800. The body-only clearance checks could not cover this.

Loose hair previously shared a cached mesh and only rotated as a whole beneath
the head. It had no contact contract with torso layers. The implementation in
progress keeps cached source hair but clones instance meshes for fitted contact.
Its support combines the actual body, selected garment shells and equipped
armor. Head and drape transforms are converted into torso/support coordinates
before contact correction; the scalp attachment stays fixed. The deformer is
called after head pose and eased hair swing, and rebuilding composition creates
new support without mutating another wearer's shared source mesh. This is a
contact projection, not a hair physics simulation or a universal collision
system. Motion, cache isolation and visual quality still need verification.

The first contact attempt passed its vertex tests but failed two stronger checks.
A triangle-center witness still crossed the torso (about -0.032 field distance),
and the native jump capture reverted to apparent holes. The latter exposed an
asset-lifetime mismatch: ordinary Builder meshes are render-only and release CPU
data after extraction, while the no-render unit harness retained it. Editable
hair now retains MAIN_WORLD as well as RENDER_WORLD, including the cached source
needed to redress an already rendered wearer. Mesh positions remain per-instance.

The fitting revision uses the same authored lock centerlines as hair generation,
reserving each lock's radius against a clearance isosurface. It translates the
lock section instead of flattening its individual vertices onto the body; this
also avoids classifying opposite faces of one lock on opposite body sides.
The existing torso ray query now delegates to a clearance-aware version with
zero clearance, preserving its prior contract for other callers. Triangle,
vertex, upload-retention and native-motion checks remain necessary evidence.

A continuous radial contact trial passed the animated triangle and vertex
regression and the static emitted-hair check. A native scripted jump also kept
hair outside the plate after renderer upload, confirming the CPU asset retention
fix. It is not accepted as the final shape: the loose-hair bounds regression
failed because a nymph's half-width grew from about 0.20 to 0.34, and the close
capture visibly flared the front locks sideways. The existing bounds assertion
is retained while contact direction is refined. The fitting must preserve a
plausible drape as well as clear the body; passing collision tests alone is not
sufficient. Full affected tests and the quick gate must be rerun once this
geometric iteration is stable.

The follow-up directional trials are also incomplete. Biasing the radial ray
from the torso center toward front/back narrowed the direction but left a face
penetration of -0.0027 at rest. Casting instead from each current lock center
avoids relocating whole sections across the chest, but the animated regression
still finds a triangle at tick 1 near (0.1961, 0.3987, 0.0194), with field distance
-0.00668. This latter trial is the current implementation. No test tolerance was
relaxed. Adjacent vertices can choose different authored centerline segments;
independent section projections therefore do not guarantee that their connecting
faces remain outside the support. The next refinement needs contact constraints
on connected mesh faces, with fixed scalp vertices and preserved source geometry,
rather than another arbitrary clearance increase. Cache-isolation coverage and
the full stable-checkpoint gate remain outstanding. Documentation and whitespace
checks passed after the radial-trial notes; later trial notes require rechecking.

A connected-face contact pass now follows the authored-lock projection. It
samples triangle centers and edge midpoints in support coordinates and distributes
outward corrections through the shared vertices using their barycentric weights;
scalp vertices have zero mobility. The bounded iteration fixes the retained
animated face witness and the static emitted-hair regression. Native rest and
jump captures confirm the post-upload deformation remains active. This is still
an experimental fitter, not accepted model quality: the native captures show
broad stretched shoulder wings, and the original nymph hair-bounds test still
fails (half-width about 0.33 versus about 0.20). The test has not been loosened.
Contact projection needs a strain/shape-preservation contract that distributes
motion through connected hair, or a better authored deformation representation;
clearance constraints alone cannot provide that. The current pass is neither
continuous collision detection nor proof against intersections between its
sample locations. Runtime cost also needs profiling before acceptance.

A separate isolation regression composes two wearers through the same asset
library, snapshots existing mesh positions, then deforms only the first wearer.
It checks that composition leaves existing meshes intact and animation changes
neither cached sources nor the other wearer. This covers the intended ownership
contract independently of whether the contact shape is aesthetically acceptable.

The next trial adds source-edge stretch constraints interleaved with contact.
The same Human/Sylvan/breastplate rest and jump views no longer show the broad
shoulder wings; an additional Nymph/native-outfit capture also shows a plausible
shoulder drape. This is stronger visual evidence but not sufficient acceptance:
a new animated edge-strain assertion fails on the first trial (source length
0.00593 becomes 0.01699). More edge-relaxation sweeps between support sweeps are
being tested without relaxing that assertion. Support-query cost is substantial
and remains an optimization requirement; the bounded solver is experimental.

The old loose-hair test's exact unfitted-bounds comparison is now applied to the
actual cached source coordinates, transformed by the same head scale and crown
pivot. Its rendered crown positions are separately compared to those coordinates.
The existing walking/turning swing and cloth-motion assertions are retained.
This replaces only the obsolete expectation that collision-fitted lower hair
must keep every bound of the intersecting source shape. Animated face/vertex
clearance and edge-strain assertions in hair_contact cover the fitted fall;
instance-isolation coverage protects both the cached source and other wearers.
The replacement motion test and stronger contact test must both pass before
claiming the revised contract is verified.

The eight-relaxation-sweeps trial still fails the unchanged strain bound at tick
zero: an edge of length 0.00830 becomes 0.02140. The source-placement/crown and
walking/turning/cloth-motion test passes in full with its retained motion
assertions. Its runtime is too high for accepting the current deformation loop.
The improved Human and Nymph captures above came from the preceding one-sweep
trial; do not claim they validate the later eight-sweep runtime. Mesh isolation
also passes in the latest trial. Convergence, current-runtime visual checks,
performance, affected suites and the quick gate remain outstanding. Future
optimization should consider reusing the prior fitted state and accelerating
exact support queries, while testing relaxation when contact is removed so a
previous obstruction cannot leave a permanent hair bulge.

### Control chains for sliding hair contact

The maintainer's direction is to deflect whole hair locks around torso, armor,
posed arms and other visible equipment, with sliding contact and secondary
motion analogous to soft-body bones. Individual mesh-vertex projection is not
the intended final representation. Major authored locks already have centerline
paths, so the implementation now starts fitting those short chains instead.
Each chain retains scalp membership, authored segment lengths and thickness;
contact is sampled at controls and quarter/mid/three-quarter segment positions.
The mesh follows the translation and rotation of its bound source segment.
The old face-contact/stretch pass remains as a temporary corrective pass and
its regression witnesses remain enabled. The new chain solver owns no materials,
render assets or species-specific anatomy; callers supply a distance field.

This is the initial posed-chain fitting stage, not yet a physical bone simulation.
It rebuilds from posed targets, while the existing eased drape drives secondary
motion. Damped persistent control state, restoration when obstacles move away,
posed arm/forearm collision shapes, and smooth blending across adjacent lock
bindings still need implementation/verification. Actual GPU skinning is an
optional representation choice; CPU control points already provide the same
small deformation control structure. A plane-contact unit test covers fixed
socket placement, thickness, segment length, tangential sliding and removal of
contact. Whole-mesh tests and native captures must still establish whether the
chain representation resolves the existing clipping/strain witnesses and improves
runtime. The initial compile exposed a Rust indexing-borrow error, corrected
before the current test build; no successful chain-runtime claim is made yet.

The first chain-based fitting checkpoint passes the plane sliding/release test,
instance isolation, the complete animated clipping-and-strain regression, static
emitted-hair clearance, the retained walking/turning and cloth-motion regression,
and the bounded focused/seeded composition batch. A rebuilt native client also
passes scripted pose replay with an equipment change during animation. Direct
rest/jump captures show front locks outside the breastplate without the former
broad shoulder wings. Some shoulder bends remain abrupt, consistent with the
current single-segment binding; this is not final hair quality. Release build,
all-target release Clippy and the repository quick gate pass. Full platform CI,
desktop deployment and native mouse checks were not run for this checkpoint
(mouse checks remain waived by the maintainer).

The successful mesh regressions include the temporary face-contact/stretch
correction. They do not prove that chains alone can replace it, or that all hair
surfaces clear every collider. The moving-arm collider, smooth segment/lock
weights, damped persistent state and runtime optimization remain open. Source
placement and fixed scalp assertions remain separate from fitted-fall clearance;
no former clipping or strain tolerance was relaxed to obtain this checkpoint.

### Continuous skinning through control joints

The first chain binding used a single rigid rotation for each source segment.
A retained synthetic surface regression demonstrates the defect independently
of collision fitting: two points 0.00002 apart on opposite sides of a bending
control joint separated by 0.02528 after deformation. The regression also checks
that a uniform rigid bend preserves the surface, guarding against a fix that
simply shrinks the radial offsets.

The revised deformer precomputes one orientation at every control, averaging the
rotations of its neighboring segments, then interpolates those unit orientations
along each segment. Both sides of a shared control therefore use the same frame,
while a point keeps its radial distance from the interpolated centerline. The
contact solver and its clearance/strain tolerances are unchanged. This addresses
within-lock segment boundaries; bindings between distinct sculpted locks still
need smooth weights, and chain centerline bends themselves remain piecewise
linear. The failing witness is retained; whole-mesh and native checks are required
before treating the revision as verified.

The continuous-joint revision passes its first-red regression, the animated
clipping/strain and isolation checks, static emitted-hair clearance, retained
walking/turning/cloth motion, and the bounded focused/seeded composition batch.
The rebuilt real client passes scripted replay with a mid-motion equipment
change. Native rest/jump captures preserve clearance; the visual difference in
this fixture is subtle, and larger shoulder angles remain. Release build,
all-target release Clippy and the quick checkpoint gate pass. Quick initially
deferred while Clippy was active and was rerun only after that process finished;
its concurrency guard was not bypassed. Full platform CI remains outstanding.

For the future moving-arm contact layer, animation already has current arm and
forearm transforms before hair fitting executes. The composed body and clothing
should contribute visual contact shapes attached to those joints, with sleeve
and armor thickness accounted for by their generators. Hair should consume the
posed shapes through the shared contact interface, not guess radii from species
names. These are visual fitting proxies, not new authoritative gameplay hitboxes.
This is a design note, not an implemented arm-collision feature.

### Exact regional contact-field programs

Hair repeatedly evaluates the same composed torso field during chain and mesh
constraints. The new ContactField reuses Sculpt's conservative operation bounds
to retain only operations that can affect a query cell. It preserves operation
order and recursively localizes union children. It caches field programs, not
sampled distances: there is no interpolation grid or relaxed collision margin.
Cells are 0.1 body-space units, bounded to 256 entries per immutable composed
surface, with rounding allowance around their bounding spheres. Queries beyond
the cache limit or coordinate domain evaluate the original program. Equipment
recomposition constructs a fresh source/cache. A lock protects the cache because
rig components must remain thread-safe; query order does not change field values.

Tests compare exact float bits against the original field for nested blends,
carvings, inflation, cell boundaries, large coordinates and capacity fallback.
Fresh caches are used for each boundary neighborhood so early cache saturation
cannot hide the culled path. A separate witness requires fewer evaluated source
operations near a body surrounded by distant layers. The animated humanoid
regression also compares cached and original distances on its emitted hair
vertices. These are correctness/work-count checks, not a benchmark result.
Native capture equivalence and the existing fitting regressions remain necessary.

Future moving arms should be separate posed contact contributions alongside the
static cached torso field. Do not insert moving geometry into an immutable cache
without an explicit revision/invalidation contract. A reusable contact scene may
combine static body programs with local-space attachment fields or analytic
capsules transformed by current joint poses. No moving-arm implementation or
quantified runtime improvement is claimed by this initial cache change.

The regional-field checkpoint passes three field-equivalence/budget regressions,
all four hair-contact tests (including the new full-field oracle), static emitted
hair clearance, walking/turning/cloth motion, the bounded composition sample and
real-client scripted replay. Both retained native PNGs—rest and jump—are byte
identical to the preceding continuous-joint build for the same input scene.
Release build, all-target release Clippy and the quick gate pass. This establishes
exactness for the exercised fields and captures, not universal proof or a measured
speedup. The current perf_compare workloads do not include the independent 3D
contact solver; a suitable harness case is needed for a formal timing comparison.
Full platform CI and the previously listed animation/contact features remain open.

Another optimization candidate is convergence: the chain currently performs a
fixed 48 sweeps, while the mesh pass can keep correcting tiny residuals near its
target clearance until its iteration cap. Any early-out criterion should check
both contact and length residuals, retain the existing clipping/strain witnesses,
and report work counts separately from formal performance measurements. This is
an investigation note; this checkpoint changes field evaluation only.

### Deferred posed arm contact experiment

An experimental implementation derived six conservative capsule envelopes per upper-arm
and forearm mesh from the composed geometry, including clothing in those meshes.
Faces are clipped to each axial slab before fitting its radial envelope; using
whole crossing triangles overestimates shoulder size. Profiles follow the current
local joint transforms and join the static torso field through a contact scene.
Its invalidation contract required refitting when only an arm moved, even when
the head stayed unchanged.
Cached source meshes retain CPU positions so profiles can be rebuilt on redressing.
This costs memory; reusable profile metadata beside the mesh cache is a future
alternative. These visual envelopes are not gameplay hitboxes.

The first animated regression exposed an arm intersection before integration.
The first integrated version then failed the existing hair edge-strain bound at
the shoulder. Tightening the envelope, doubling mesh relaxation sweeps, and replacing rounded
ends with capped cylinders each still failed the existing edge-strain bound.
The experiment was therefore removed from the active source; it is not a
validated moving-arm solution. The existing strain and scalp-attachment bounds
remain in force. Conservative capsules do not describe concave garments exactly,
and separately attached sleeve drapes are not yet contributors. Current posing
assumes rigid joint transforms; arbitrary joint scaling needs explicit handling.

The hair controls currently solve a posed geometric fit, not persistent physical
motion. Head/drape motion supplies the animation target. Future inertia, damping,
and tangential friction belong in a reusable chain solver, with collision shapes
provided by body/equipment composition. Keep fixed sockets, thickness, length,
and contact constraints separate; a single hair bone cannot wrap a lock around
several independently moving surfaces. Continuous collision detection and contact
history are also future work, not guarantees of the current sampled constraints.

The active Rust and Cargo inputs were restored byte-for-byte to the preceding
regional-field checkpoint, verified against its recorded SHA-256 inventory.
The experiment and first-red/iteration logs are retained locally for diagnosis.
The earlier passing build, fitting, native replay/capture, Clippy and quick
results therefore apply to the restored source. No new arm-contact acceptance
claim is made. A subsequent implementation needs to resolve competing shoulder
contact, fixed-socket and mesh-strain constraints together; increasing iteration
counts alone did not resolve the observed failure. Preserve the pending arm
clearance regression when reintroducing the feature, alongside all existing
scalp, strain, body-contact and per-wearer isolation checks.

A diagnostic rerun of the capped-cylinder experiment narrowed the first failure:
both edge endpoints were movable and bound to the same curve/segment/t value.
Their fitted contact gaps were positive (approximately 0.0077 and 0.0087), while
the local edge grew from 0.00567 to 0.01632. This particular failure is therefore
not explained by a fixed scalp socket or switching between hair controls. The
solver currently relaxes edges before face contact and exits after contact;
its final face corrections can leave unrelaxed edge strain. A balanced ending
length pass is being investigated, with contact assertions retained to detect
whether length restoration pushes the surface back into the obstacle. This is
a diagnosis and hypothesis, not yet an accepted fix.

Follow-up experiments confirmed that the constraints compete. Eight final edge
sweeps moved the first strain failure from jump tick 0 to tick 3. Up to 64 sweeps
with early exit removed that strain failure but produced an arm intersection at
tick 3. Projecting near-contact edge corrections into the surface tangent plane
then progressed beyond those failures, but the test failed its independent
requirement for more than 20 front-torso hair witnesses. The failure did not
report its tick, so no all-pose clearance claim is justified. This indicates a
coverage/shape-retention problem requiring rendered diagnosis, not permission to
remove the witness. The tangent version also introduced many normal queries into
edge relaxation; no performance improvement is claimed.

The next solver design should explicitly balance authored-shape retention with
contact and length constraints, rather than allowing mesh corrections to choose
an unconstrained route around the shoulder. Consider control-chain targets and
bounded displacement/curvature together before relying on expensive mesh-level
corrections. The experiments and diagnostic logs are preserved locally. Active
Rust/Cargo inputs were again restored exactly to the validated regional-field
checkpoint; neither the arm experiment nor its diagnostics are enabled in the
normal client. Existing regression witnesses remain unchanged.

Rendered follow-up established that the coverage loss is a real route change,
not merely a ray-sampling artifact. The diagnostic 16 ms jump test had 208
front-torso witnesses at tick 20 and zero at tick 21. Native 50 Hz captures around
the corresponding time show front locks still present at ticks 16 and 17, then
moved behind the shoulders at tick 18 as the arms swing forward. The time steps
are different, so these are neighboring-pose evidence rather than exact frame
identity. Both capture runs completed; front and side views are retained locally.

The stateless fitter restarts each control chain from its authored target every
pose. This permits a different collision-avoidance route to be chosen abruptly.
A per-wearer history experiment now seeds the next fit from the previous fitted
chain transformed into the current head/drape frame, with a small authored-target
pull during relaxation. Recomposition starts fresh history, and arm-only changes
must invalidate placement without discarding the prior fit frame. This is under
validation; it is not a claim of inertia, continuous collision detection, or a
frame-rate-independent physical model. Acceptance still requires shape retention,
obstacle release, isolation, contact and strain checks, plus rendered motion.

The history experiment did not pass acceptance: it retained 374 front witnesses
at jump tick 21 but fell to zero by tick 24. A second experiment used the initial
fitted chain, transformed into hair-local space, as the subsequent resting target
instead of the raw authored curves inside the body. It retained 245 witnesses at
tick 21 but only 18 at tick 22, below the unchanged required count. Neither
variant is accepted. Their source, failed tests and completed native captures
are retained locally; active Rust/Cargo inputs are restored to the prior verified
checkpoint. Shape retention and history alone did not establish a stable route.

Future investigation should inspect the continuous path between fitted poses,
not only endpoint signed distances. Discrete node and segment samples can select
a new side of an obstacle without maintaining the prior drape. Per-frame vertex
and control displacement diagnostics in scripted review would expose such jumps
directly, alongside existing joint-pose evidence. A swept-contact or constrained
path method needs its own continuity, contact-release and frame-step tests before
integration. The conservative arm profiles also still need a geometric accuracy
review; the present evidence does not prove that collision-envelope error is
irrelevant. No production moving-arm fix or performance result is claimed.

### Current work in progress: posed contact coordinates

The posed-arm prototype is now retained in the isolated working tree for further
iteration; it is not an accepted runtime checkpoint. The normal client executable
remains the last validated baseline. Do not infer that source and executable now
contain the same experimental feature, and rebuild explicitly before new native
captures. Nothing has been published or deployed.

The contact-profile regression failed on an end-boundary face point even before
animation. Its correction pads both axial ends by the same 0.003 allowance used
radially. Posed cylinders now retain their joint-local geometry and evaluate
queries through the inverse joint transform and support-coordinate scaling.
Transforming endpoints and reconstructing a round cylinder after nonuniform
support scaling did not preserve that geometry. Distance values use the minimum
support-space scale for the current rigid joint-transform contract; arbitrary
joint scaling still needs a general distance-bound treatment. The sparse-face
coverage and composed-joint pose regression passes with this correction.

The corrected prototype also passes the chain and per-wearer isolation checks,
but still fails the jump test's front-hair coverage requirement. The route-change
problem is therefore unresolved; correct contact coordinates alone are not its
solution. The failing acceptance test remains intact. The prototype currently
uses stateless chains with tangent-constrained mesh length corrections, not the
rejected history/rest-target variants. Those variants remain archived locally.

When restoring experimental source files, refresh their modification times to
force Cargo to reconsider them; copying historical timestamps can otherwise
reuse a prior artifact. The current corrected experiment was compiled after
explicitly refreshing all restored Rust inputs. Prior native restoration was
independently checked by byte-identical rendered output, not source hashes alone.

### Work in progress: control correction paths

A focused regression demonstrates that an unconstrained control correction can
jump across a spherical obstacle while both endpoints are clear. The prototype
now advances correction paths by signed-distance bounds, stops near contact and
projects remaining movement into its tangent plane. The loop is bounded (four
slide passes, at most 512 advance queries each); exhausting the budget returns
the last reached point rather than an unverified destination. Initial overlaps
use a separate bounded depenetration step. This is a static-field method; it is
not continuous collision detection for moving arm contributors, and guarantees
also depend on the supplied field's distance-bound accuracy.

The crossing regression failed before this implementation and now passes,
including a wall-sliding case and unchanged unobstructed motion. Existing chain
socket/thickness/length and shared-joint continuity tests pass, as does per-wearer
mesh isolation. Full humanoid acceptance remains failing: with this correction
wired into chain relaxation, the jump fails an existing edge-strain bound at
tick 3 (rest length about 0.00952, fitted length about 0.02628). No strain or
clearance limit was relaxed. This implementation is still part of the isolated
prototype, not a replacement for the validated normal executable.

Initial fitting and motion of an already valid fit need separate contracts.
The authored curves may start inside the composed body and require substantial
fitting; treating those corrections as motion from a valid collision-free state
changes the solution. A follow-up should establish and validate the initial fit,
then apply path constraints with pose history and moving-contact handling.
The primitive success alone does not validate that integration. Runtime cost,
query-budget exhaustion, contact release, motion continuity, and native rendered
acceptance remain required before accepting this work.

### Work in progress: separate initial fit and advancement

The chain module now exposes `fit` for initial overlap resolution and `advance`
for continuation from prior fitted controls. Both use one length/contact solver;
only advancement applies path-checked corrections and a small target-restoring
step. A new regression failed with stateless advancement, which jumped a control
from one side of a sphere to the other. It now passes, including recovery toward
the target after the obstacle is removed. All four focused chain tests and
per-wearer mesh isolation pass.

HairRig retains previous fitted controls and their frame independently of the
unchanged-pose cache. Arm-only pose invalidation clears that cache without
throwing away history. The initial fitted chain is stored in hair-local space as
the resting target; model recomposition creates fresh state. This is kinematic
fitting with solver-iteration target restoration, not an inertial or generally
frame-rate-independent physical simulation.

The full jump test no longer stops at the preceding tick-3 strain failure, but
still fails its unchanged requirement for more than 20 front-hair witnesses.
Acceptance is therefore incomplete. The normal executable is still the validated
baseline; the isolated source is a prototype with a failing integration test.
A diagnostic comparison of control displacement against final mesh displacement
is needed next: the chain retains history, while face/edge correction still
restarts from the skinned source mesh each pose. Either stage could be responsible
for the observed route change. Do not assume which stage is responsible without
measuring both. Moving-contact sweeps, runtime cost and native acceptance remain
outstanding.

### Investigation: jump contact displacement and arm timing

The stateful prototype's 16 ms jump diagnostic localizes the route change to
control fitting, before mesh correction: maximum control displacement rises
from about 0.00735 at tick 20 to 0.08266 at tick 21 and 0.14788 at tick 22.
Maximum final mesh displacement is about 0.08519 and 0.13822 at those last two
ticks. Front-hair witnesses fall from 158 to 4 at tick 22, failing the unchanged
coverage requirement. These are support-space distances for this fixture,
not a general performance or physical-accuracy result.

The shared jump arm pose derives lift from the square root of the parabolic
body height. Although position is continuous at takeoff and landing, angular
velocity is unbounded at those boundaries. A regression is being added for
bounded arm travel at supported review rates and near both phase boundaries.
This is an animation defect candidate independent of hair; fixing it does not
by itself prove that moving-arm contact or hair route retention is correct.

The new actual-client regression reproduces the defect using the previously
validated executable, a male Human in Knight clothing, and 50 Hz captures.
Across ticks 17 to 18, the left hand moves 0.42261 world units in 20 ms; across
ticks 39 to 40 it moves 0.31241. The right hand shows the same spikes. The
regression fails its 0.1-unit per-frame limit. This isolates the visible arm
motion issue from the long-hair solver. The first harness invocation did not
reach the test because an unrelated workspace build failed with invalid core
metadata; the successful reproduction used the explicit existing 3D executable
and renderer-only binary-directory override. The focused Rust arm-speed test
was still compiling when this evidence was recorded; no animation fix or
acceptance pass is claimed yet.

The focused Rust regression also failed before the animation change (20 Hz,
phase 0.26087, arm angular speed 14.30225 radians/second against a 14-radian
bound). Shared biped jump arms now use smooth anticipation, overhead lift and
recovery envelopes instead of square-root body height and direct crouch factors.
The arm swing begins before takeoff and recovers through landing, preserving
the overhead pose while removing singular boundary speeds. Body lift, leg
contacts, torso and head timing are unchanged. Validation of this change is
pending; the native executable still predates it. The new process regression
checks both hands around takeoff and landing, with actual rendered captures.

After the curve change, the focused arm regression passes at 20, 50, 60, 120
and 240 Hz, including fine samples across both flight boundaries and the
retained overhead pose. Python process-test syntax, documentation checks and
diff whitespace checks pass. The full armored-hair jump still fails: front
witnesses fall to 7 at tick 19, when anticipation has brought the arms forward
earlier. Maximum control displacement on that frame is about 0.07969 and mesh
displacement about 0.09005 support-space units. Thus smoother arm animation
addresses an independently reproduced defect but does not establish hair
contact-side retention. Remaining jump regressions and the client rebuild were
still active at this checkpoint; native validation of the new executable,
Clippy and quick verification have not yet been completed for this change.

### Next contact boundary: moving contributors and attachment transport

The current `Scene::set_pose` replaces all arm contributors with their final
pose, retaining no trajectory. `chain::advance` checks control corrections
against that final static field. It therefore cannot distinguish an arm that
crossed a lock between frames from an arm already on the other side. In
addition, HairRig transports all prior controls through the head-frame change
before asking the solver to check correction paths. Those two unchecked motions
remain separate from the now-bounded jump arm curve.

The next focused regression should move an obstacle through an initially clear
control while leaving both endpoint configurations clear; stationary-obstacle
correction tests cannot cover that case. A shared solution needs previous and
current contributor transforms with stable part identity, and should advance
socket motion and obstacle motion together. Prefer rigid translation/rotation
interpolation and a movement bound based on contact thickness over blindly
interpolating affine matrices or adding an outfit-specific offset. Shape changes
from recomposition need an explicit fresh-fit/reset contract. Frame-rate
comparison and query cost must accompany integration; existing full-mesh strain,
clearance and front-coverage assertions remain required. This is a proposed
contract, not an implemented moving-contact guarantee.

Native validation rejected the first smooth curve despite its passing angular
unit test: the left hand still moved 0.13408 world units at tick 17, above the
unchanged 0.1 bound. Lift is now spread over more of the jump, with a shorter,
smaller backswing; both regressions are being rerun without changing their
limits. The broad six-test jump run was deliberately stopped after reproducing
the hair failure and the native motion failure, before editing its Rust inputs.
Its unfinished belt/other checks are incomplete, not passes. The queued build
then completed, and no Rust inputs were edited until all prior build/test
processes ended. The normal executable has now been rebuilt from the prototype;
it is no longer the older validated baseline and must not be presented as an
accepted full hair/contact result.

The wider arm curve passed the angular test but native hand displacement still
failed at 0.10379 units on tick 18. Inspecting all captured transforms rather
than just the first assertion exposed a second singularity: at landing tick 41,
the head moved roughly (0, -0.08510, 0.07510) and the left hand roughly
(0.00174, -0.18097, 0.07371). `jump_profile` used fractional powers 0.7 and 0.45
for crouch/landing phase, with unbounded phase derivatives at their starts.
These now use the shared smooth easing function before the sine, preserving
peak crouch depths and flight timing. A focused boundary-speed regression also
checks retained crouch and landing depth. Native hand-displacement limits remain
unchanged and supply the pre-fix actual-process failure for this correction.
The revised shared body curve is awaiting unit/native validation; earlier hair
failure results remain evidence for earlier inputs, not a final result for this
new curve. Broader belt/contact checks still need a complete rerun.

With both arm and crouch curves corrected, both focused Rust regressions pass
and the unchanged real-client hand-displacement test passes across the captured
takeoff and landing boundaries. The release executable builds, and documentation
and whitespace checks pass. This confirms the shared jump-motion correction for
those witnesses; it does not validate the experimental hair solver. A fresh
armored long-hair regression and nearby native front/side captures are running
against this new body curve. Broader interrupted jump tests, Clippy and the quick
checkpoint remain outstanding; no push, publication or acceptance is claimed.

Final evidence for this jump-curve iteration: the fresh armored-hair test fails
its unchanged front-coverage condition at tick 15 (9 witnesses; more than 20
required). Native capture completes at 50 Hz for ticks 16/17/18, front and side.
Visual inspection shows shortened/retracted front locks and an abrupt hair-shape
break around the neck/shoulder; the hip-layer edges also remain conspicuous.
The native capture rate differs from the regression's 16 ms step, so equal tick
numbers do not identify equal poses. These images support keeping hair fitting
experimental; they do not negate the independently passing jump-motion tests.
Do not accept this as a full model-quality checkpoint. Next work remains moving
contact history/transport, then full mesh and visual acceptance, runtime cost,
and completion of the interrupted broader verification.

The quick checkpoint completed successfully (format, architecture, build tools,
Python tools); its selection included no core Rust packages or core process
tests. Release all-target Clippy also passes. These checks do not cover or
supersede the recorded failing 3D hair acceptance test.

A separate attachment-boundary concern emerged from the side capture:
`hair_contact::spawn` assigns a Boolean movable mask using posed vertex height
0.58, and `place` either applies the chain deformation or leaves the vertex
fixed. A chain segment can interpolate a nonzero displacement at that height,
so the switch has no continuity guarantee. The current shared-control skin test
covers continuity between two segments of one chain, not this fixed/deformed
boundary or bindings between different locks. A dedicated regression should
measure neighboring surface points across the attachment boundary under a bend.
A continuous attachment influence would need consistent weighting in skinning
and contact/strain corrections while retaining exact scalp attachment. This is
a separate hypothesis for the visible neck break, not proof of the front-lock
route failure or authorization to relax the existing tests.

## Live renderer playback and deterministic capture

The live playlist runner uses the validated composition/event schema with separate
camera holds and eased transitions. One window persists. Composition variants
preload once and reset on scene entry; hair, robe and cloak caches are invalidated.
This bounded review cache is not yet a gameplay inventory streaming system.
Wall-clock animation uses bounded substeps and reports discarded long-stall time.
CPU update/render intervals measure pacing; actual GPU/display timing remains
future work. Fixed-tick capture retains screenshot barriers. Future refactoring
should share stage/lifecycle code while preserving both clock contracts and their
actual-process tests.

### Attachment transport contact boundary

The head-frame advection path now sends free controls through the shared static
contact sweep before relaxation. Pinned socket controls still follow their
exact authored destination. A clear-endpoint sphere regression first reproduced
tunneling under the previous direct transport, then passed with the sweep; an
unobstructed-motion witness verifies exact destination recovery. All six chain
checks and release all-target Clippy pass for this change.

The full armored Sylvan Duelist jump witness remains failing at tick 15 with
9 front witnesses. The similar failing pose after constraining transport means
this correction is not a solution for the observed front-lock retraction.
Moving-arm history, attachment-boundary continuity and deformation/length behavior
remain separate unresolved contracts. In particular, current contact queries
still use the final posed arm field, not a temporally swept contributor field.
Native visual acceptance and full model-quality acceptance are not claimed.

Per-wearer mesh/source isolation passes after the transport correction. A rebuilt
native client completed eight front/side captures at 50 Hz (ticks 0, 12, 13 and
18). The side views show a conspicuous neck/shoulder discontinuity already at
rest; this is not solely a jump-pose defect. The existing Boolean fixed/deformed
vertex boundary therefore remains a concrete independent regression target.
These images do not establish that boundary as the only cause, and the 50 Hz
captures are not tick-equivalent to the 16 ms integration witness.

### Continuous scalp attachment influence

Hair mesh attachment now stores a neutral-frame scalar influence instead of a
Boolean movable mask. The existing fixed scalp boundary at height 0.58 is
preserved, with smooth influence increasing to one at 0.50. Skinning, face-contact
mobility and edge-strain mobility consume the same value. Animated height does
not reclassify attachment, and zero-influence vertices retain their exact source
position. These measurements remain prototype authoring data; a shared body
fitting description should eventually provide the attachment region.

The focused continuity regression first reproduced a 0.14730959-unit split
between neighboring samples across the former cutoff. It and the exact-scalp/
free-region witness pass with continuous influence. Chain and per-wearer source
ownership witnesses also pass. This proves the tested skinning boundary, not
all final-mesh continuity or visual quality. Cross-lock binding transitions and
contact corrections still require emitted-mesh and visual review.

The completed jump rerun still fails at tick 15 with 9 front witnesses. No
coverage, clearance or strain limits were relaxed. Both the transport and
attachment corrections leave that full-motion acceptance issue unresolved;
rest-target orientation and moving contact history remain separate candidates.

The rebuilt native capture completed all eight requested views with this scalar
influence. Resting and posed side views still show the collar-like neck bend, so
this change is not accepted as a visual solution for that defect. Source review
confirms that authored strands are smoothly unioned into the hair surface, then
vertices are bound by nearest segment after meshing. Preserving/blending those
source influences is the next structural boundary to test; a single nearest
segment can switch between differently deformed locks. Mesh normals are already
recomputed after deformation. This finding does not establish cross-lock binding
as the sole cause of the remaining visual defect.

## Modular worn-equipment studies

The six November 2026 studies specify 39 physical appearances: eleven hats and
helmets, two shirts, ten shoes and boots, four gloves, and twelve outerwear
recipes. These are appearance recipes, not magical item identities. Fixed and
shuffled appearances must remain distinct concepts; a conical hat, for example,
does not identify which power the server assigned it.

The first implementation slice adds headwear and footwear recipes to the existing
independent equipment composition. Preview names are the hyphenated study names
(for example `fedora`, `crested-helmet`, `hiking-boots`, and `snow-boots`). Live
selection uses palette-approved `item.armor.head.<name>` and
`item.armor.feet.<name>` assets in the corresponding disclosed worn socket.
Existing `helm` and `boots` previews retain their original recipes. No scenario
stock, save schema, or simulation eligibility changes are part of this slice.
The centaur has an explicit headwear attachment adapter; it does not acquire
humanoid footwear or a full wardrobe merely by having a humanoid upper body.

### Layering and remaining work

- The second slice adds independent shirt/glove selection and wrapping, mantle,
  hooded cloak, fitted robe, and apron topology families. The 39-look catalog is
  addressed through `item.armor.shirt.<name>`, `item.armor.hands.<name>`, and
  `item.armor.cloak.<name>` in addition to the first-slice head/feet assets. A robe
  and apron occupy the cloak gameplay slot despite their different geometry.
  Centaur hand anchors can display gloves without granting torso or boot slots.
- Short-sleeved shirts retain covered shoulders independently of exposed arms;
  actual armor coverage suppresses shirt prints/collars. Glove cuffs fit retained
  sleeves. Study capes have open-front cuts, distinct fullness/hem/folds, lined
  interiors, and optional shoulder mantles. Adult feminine cuts remain linked to
  the resolved body, not a replacement standardized torso.
- Wrapping tails attach to an emitted torso winding using the same retained
  shirt/armor support as the upper wraps, then ease toward the skirt clearance
  envelope without moving their authored hem. Raising the hip-local root alone
  does not establish depth contact. Native apron panels now use retained posed
  leg triangles for local outward contact, preserving waistband pins, hem height
  and paired thickness. Robe overlays republish their solved baseline even on
  unchanged poses before apron contact; deformation never accumulates. Exact
  review walking/strike witnesses and native views cover the repaired knee
  penetration. Directional clearance is not a general cloth simulation; demands
  conflicting with pins or the bounded shape allowance are reported as fallbacks.
- Built-in boot shafts fit retained calf/trouser triangles by direction,
  with cuffs and buckle bands following the leather. Complete clipped support
  cells bound sidewall faces, including support between profile rows/rays. The
  foot last, sole and equipped-footwear path stay independent. The Barbarian
  study's fur-topped boots use the same fit: underfur and each lock root follow
  actual leather at their own height, while shoulder fur keeps its old recipe.
  Human native and Barbarian neutral/walk/strike views qualify the sampled
  silhouette, not every wardrobe or arbitrary Euclidean clearance at shaft caps.
  Other future fringe kinds remain on their existing path until qualified.
- Moving layered garments need explicit contact, not merely a larger rest mesh.
  Retain emitted skirt surfaces and their current transforms for robe fitting.
  Apron/wrap meshes above independently deforming robes must be instance-owned
  and follow the solved outer surface. Empty support lists preserve legacy fits.
  Numeric and native-motion regression tests must verify both layer directions.
- Study-cloth shoulder turnover derives from retained torso/garment/armor and
  arm support rather than an oversized authored ellipse. Preserve independent
  hanging fullness. The contact solve moves roof vertices vertically and lower
  turnover/hanging vertices radially, with bounded collar attachment and swept
  face certification. Neither collision clearance nor numerical convergence
  alone accepts the resulting shape: retain height, width and slope gates.
  Treat arms as finite swept collision obstacles, separately from inner cloth
  that requires outermost containment. Establish outside membership using closed
  mesh components; a distant arm ray hit alone must not expand the garment.
  Open decorative faces still participate in collision, not solid membership.
  Cache only unchanged face queries within one pose solve, including the arm
  domain in the key; domain growth must requery obstacles. Compare cached and
  uncached geometry and failures exactly before claiming a runtime improvement.
- Resolve shirt, armor, then outerwear before generating geometry. Armor may
  completely conceal a shirt, allowing hidden geometry to be omitted. Preserve
  visible collars, sleeves, necklines and hems. A cape usually leaves underlying
  gear visible, including during motion, so it must not discard those layers.
  Hidden surfaces can still be required for fitting and contact.
- Preserve adult feminine cuts and the existing body variation. Fit the item to
  the wearer rather than replacing the wearer with a generic body. Keep the same
  recognizable item design across body variants.
- Add further visual attachment adapters where an eligible creature needs them.
  A supported renderer socket is not an authority to wear an item; live clients
  consume only the server's disclosed worn list.
- **Future mob definitions need anatomy and equipment-slot information.** The
  authoritative definitions should describe body parts, size/form restrictions,
  slot availability and wielding constraints. Keep that simulation contract
  separate from visual head/hand/torso/foot anchors and fit dimensions. Distinguish
  putting equipment on from retaining it through a body transformation.

### NetHack reference boundary

The researched baseline is NetHack 3.6.7, specifically
[`worn.c`](https://github.com/NetHack/NetHack/blob/NetHack-3.6.7_Released/src/worn.c),
[`mondata.c`](https://github.com/NetHack/NetHack/blob/NetHack-3.6.7_Released/src/mondata.c),
and [`steed.c`](https://github.com/NetHack/NetHack/blob/NetHack-3.6.7_Released/src/steed.c).
It is a gameplay reference, not code to copy or an implemented simulation rule.
Monster armor use is not simply humanoid-only: hands, animal status, body size,
body shape, horns and wielded weapons affect individual slots. Centaurs and
slithy forms cannot wear boots; horns conflict with rigid helmets; two-handed
weapons conflict with shields. Torso armor has additional fit rules. Saddles have
a separate eligibility rule and are outside these six studies. NetHack 3.7
changes some details, including giant mummy wrapping, so later adoption must name
its intended edition rather than mixing rules silently.

### Verification checkpoints

Each slice needs palette/disclosure and unknown-slot tests, reversible independent
selection, head/loadout cache identity, fitting and emitted-geometry tests, and
native review in motion. Include varied builds, adult feminine silhouettes,
non-biped adapters, fully hidden and partly exposed shirts, open capes, and
walking/attacking/turning/fallen poses. Source and numeric geometry checks alone
do not establish visual quality or whole-motion clipping correctness.

The deterministic review pack lives in
`crates/client-3d/scripts/fixtures/worn-equipment-review/`; regenerate it with
`python crates/client-3d/scripts/generate_worn_equipment_review.py crates/client-3d/scripts/fixtures/worn-equipment-review`.
Its six bounded scene scripts cover all 39 appearances and layered compositions,
including centaur head/hands, robe over a full skirt, and apron over a caster robe.
Each requests front/rear neutral, walking, turning, striking, and individual
close-up captures (77 images across the pack). Actors stand on the review floor
at y=-0.5. Render
with the native client `--scene-script FILE --capture NEW_DIRECTORY --offscreen`.
The index identifies each actor's appearance and body. Generated scripts and
schema tests are preparation, not evidence that these captures have been rendered
or visually accepted.

### Continuing model and fit refinement

The user requested continuing refinement after reviewing the native renders.
Catalog coverage, green collision tests, and sampled pose passes are milestones,
not overall art acceptance. Compare each item both alone and worn against the
study, then inspect its silhouette, construction, material detail, attachment and
motion. Preserve body variation and visible underlying layers while iterating.

The November 7 review identified these concrete priorities:

- Fit cape collars around the neck opening rather than over a capped torso
  support. Remove raised shoulder shelves and keep the drape outside moving
  arms without turning the cloth into sleeves or oversized boxes.
- Seat headgear on the actual uncovered skull, not a suppressed scalp-hair
  envelope. Bound both minimum clearance and maximum cavity gap at the brow,
  roof and guards, preserve actual ears, and check the whole rear attachment
  rather than only a central contact. Review a plain skull cap and a visor on
  human and centaur builds before treating a shared fit profile as established.
- Align visored-helmet sight openings to the actual eye positions. A ray at an
  assumed brow height is not an eye-clearance witness.
- Replace shirt shoulder-pad shapes with continuous short cloth sleeves. Give
  Hawaiian shirts a readable folded collar, placket, buttons and sleeve pattern;
  retain the adult feminine tied and fitted cuts and armor occlusion.
- Rebuild helmet cheek guards around cheek and jaw surfaces, improve feather
  clumps and crest bristles, and make etched-metal patterns readable at worn
  scale. Recognizability alone does not establish faithful construction.
- Improve shoe tongue/lacing/quarters, instep-to-shaft continuity and deliberate
  calf ease. Review side and rear silhouettes as well as frontal fit.
- Reduce glove cuffs that overwhelm hands, especially centaur fencing gloves,
  and distinguish quilting, seams and cuff shapes at gameplay scale.

Keep an independent positive shape witness alongside clearance tests: an
arbitrarily inflated item can pass collision checks while looking wrong. Treat
failed native tests and visibly poor renders as work to correct, not as reasons
to loosen the expected fit. Fresh rendered comparisons are required after each
coherent correction; continue improving the remaining catalog after an initial
slice is merged.

### Shared ear attachments

Face ears, pointed-ear roots and jewelry use one parametric ear anatomy in the
head-local frame. Lobe mounts are bounded queries against the selected face
surface, not independent coordinates in each outfit. Gem drops, native drops
and hoops retain their dimensions and downward orientation; only their mounts
follow the anatomy. Missing ear support rejects the mount rather than selecting
an unrelated cheek or neck surface. Final head presentation transforms remain
outside this profile, shared by face and jewelry.

Focused checks cover moved/scaled anatomy, both sides, unsupported ears,
body/sex/build variations, sampled emitted triangle clearance, and outfit/helmet
cache swaps. These are bounded fit witnesses, not continuous collision proofs.
Keep the existing recipe ownership fix separate from fitting: selected outfits
replace native jewelry, and equipment suppression/removal restores that selection.

### Reconstructed leaf-crown ownership and shared brow frame

The Nymph's native 14-leaf crown is selected by the outfit, not emitted by
species anatomy. Selecting other headwear replaces it; removing equipment
restores it. Pointed ears remain anatomical, and both body variants retain
native leaf-crown selection.

A shared radial brow attachment profile exposes its center, front drop,
clearance and fallback radius. Leaf crowns, wreaths and circlets query existing
face/hair support in the same pre-presentation head frame. All leaf roots are
resolved before any leaf is emitted, so new ornaments cannot become their own
support. This is generation-time fitting, not triangle collision in animation.
Species silhouettes and individual ornament styles are still distinct.

### Reconstructed shared fitted strap result

Humanoid baldrics, Centaur baldrics and apron shoulder straps share the fitted
strap result. Authored knots and widths remain body/style-specific. The result
preserves the explicit edge policy: humanoid wrap-around baldrics clamp to the
silhouette, while other sewn strips end at unsupported rows. Contiguous strips
retain gaps rather than bridging them.

Hardware queries the same fitted centerline and normals as its strap. The
humanoid's rectangular buckle and Centaur's spherical clasp remain distinct
recipes. `at_height` selects the first crossing of a sloped contiguous strip;
it does not place hardware on horizontal paths or inside gaps. A future
arc-length selector is required for looping or horizontal accessories. This
extraction does not itself define torso anatomy and does not claim a performance
gain; the shared core below supplies anatomical landmarks separately.

### Reconstructed torso core and named landmarks

An anatomy-owned torso core supplies the waist, ribs and upper-chest dimensions
used by both humanoid and Centaur torso construction. Named shoulder seats and
chest/waist anchors derive from those same dimensions; baldric recipes use them
without taking ownership of the anatomy. Cloth fitting still uses the actual
layered support, not a generic chest plane. Centaur clasp height follows its
selected strap knot.

The constructors preserve body-family and sex-specific proportions. Bust contour,
volume and asymmetric fullness remain separate. In particular, the biped torso's
rib envelope may widen with the waist while the bust retains its original
shoulder-based chest wall. Garment ease, style and coverage are not anatomy
parameters. This is a bounded reconstruction for these torso families, not a
universal equipment/slot framework or a performance improvement claim.

### Apron shoulder anchors and continuous straps

Apron straps use the torso core's upper-chest height and inboard shoulder seat,
while the bib recipe keeps its own width and material. Front and back strips
share the same grown garment support. Their fully supported terminal rows are
joined across the shoulder with exact shared seam endpoints rather than left
as separate upright tabs. Missing end support leaves the bridge absent; the
neck opening and the opposite shoulder are not bridged.

This addresses the shoulder attachment only. The lower apron panel, knee fit,
and visibility of underlying baldric hardware remain separate concerns.

### Retained bounded strap-layer core; drape integration deferred

A fitted native baldric retains its resolved cloth rows and an analytic obstacle
support together. The support is bounded in explicit projection frames: one
local patch for each authored front/back segment and separate shoulder/waist
turn patches. These are conservative finite footprints clipped from the same
support surface used to fit the strap, not globally inflated torso geometry or
animation-time triangle collision. Shared ribbon thickness stays inward.

The biped figure resolves this recipe lazily, and rendering uses its retained
rows. Equipped-shirt support remains in the composition. Emission exposes the
retained obstacle for future consumers without solving the strap a second time.
This core does not change apron support or claim a visible fit improvement.

Production apron integration is deliberately deferred. Native review rejected
a hard obstacle union because it produced diagonal ridges. Subsequent blended
and ramped candidates failed continuity or off-strap locality tests. Those
experiments and failure evidence are preserved separately, not enabled behind
an untested production path or hidden as ignored tests. A future drape consumer
needs a local transition that clears the obstacle without adding unrelated
surface lift, verified on the actual emitted garment and fresh native views.
Cowl geometry and protruding hardware remain separate modeling concerns.
No measured performance improvement or universal collision guarantee is claimed.


## Conservative whole-detail armor coverage

This mask requires a containing solid that represents actual rendered armor, not anatomy or generic fitting support. It conservatively certifies the complete resolved detail bounding volume inside that solid, then certifies a margin-expanded projected rectangle ahead of the outermost detail depth. Interval upper bounds across cells, not point samples, are authoritative. Holes, partial coverage, detached slabs, invalid frames and exhausted budgets retain the complete detail. It is not a thin-cloth occlusion mask.

The female Hawaiian tie previously lost individual quads under partial armor. The intended first-red emitted 636 vertices versus the uncovered complete 1416. The consumer now resolves the whole knot and both tails without armor clipping before making one conservative all-or-nothing decision. Other shirt prints/collar clipping are unchanged.

The baldric's framed buckle bounds include rim and tongue relief; only equipped armor is considered. Synthetic enclosing-armor tests establish the consumer behavior. Ordinary armor often leaves the buckle outside because the strap was fitted to it; no frequency-of-removal claim is intended.

Apron bib integration is deferred. No notch, drape or existing apron hardware overlap is claimed fixed. Queries run during construction, not animation frames; no performance improvement is measured or claimed.

A broader collar regression exposed a conservative false negative from bounding
all curved tie pieces together: every actual vertex was at least 21.3 mm inside
armor, and all 472 triangle bounds individually certified. The whole-detail query
now accepts actual emitted component bounds sharing one global budget, still
requiring every component to certify before removing any part. This avoids empty
space in a curved detail's global box without reducing margins or weakening
containment. The existing enclosing-armor acceptance remains unchanged. These
bounded construction-time certificates are not runtime triangle collision or a
measured performance improvement.


## Shared support-fitted Centaur waist band

Current baseline measured from the emitted fixed-radius tube, using 144 independent radial mesh rays per sex/build against bisection roots of the actual torso sculpt. Maximum radial gaps: male49.729mm, female51.945mm. All three build variants share the existing waist dimensions; this work does not invent new build dimorphism. First-red accepted maximum12mm and failed as intended.

The body-support helper extraction preserves original lower root, waist/chest/neck blends and male versus bust branches. Production torso emission and waist band now share that actual support. Existing biped capped-band geometry is exposed through WaistBandProfile: authored heights, tilt, clearance, thickness and explicit support-to-emission Y translation. Centaur uses identity frame, original15..55mm height and leather material. No proxy tube radii or fake HumanoidBody.

Scope: this is a shared vertical-axis waist-band profile, not arbitrary rotated attachment frames or general cloth collision. Existing radial projection and40mm clearance bracket assumptions remain for supported torso/hip consumers. No runtime performance claim. Native waist/horse junction, seams and actual support clearance still require qualification.

Selected local tests passed: 22 distinct witnesses, including existing biped
belt/pouch and Centaur contracts. The candidate outer-belt radial gap measures
6.985–9.357 mm across these sexes/builds, versus baseline maxima49.729–51.945 mm.
Full emitted triangle vertices, edge midpoints and centroids meet the2–8.5 mm
field-clearance bound; authored height, closed seam, end-cap winding, thickness
and support/emission translation controls pass. Native visual review is recorded
separately; these measurements are not a universal cloth-fit or speed claim.

### Reconstructed Centaur forearm attachment

Shared RadialBandProfile now serves waist bands and Centaur forearm bracers from actual anatomy support, retaining sex/build shapes and authored span. Local28 focused tests and40 paired native views qualify the narrower bare-bracer fit, not glove composition. Riding-glove cuff flare remains loose; actual inner-cuff gap reaches20.215 mm across tested glove styles. No apron/notch fix.

Next priority: configurable angular/axial tessellation with measured clearance and native quality tradeoffs. Current correctness prototype is2,496 triangles per band versus80 in softened old prism; do not describe it as a performance improvement. Keep checkpoints before long builds and preserve baseline visuals.

### Radial mesh budget follow-through

Forearms now use an explicit24-sided,6-row budget:672 triangles per band instead of2,496 in the fit prototype. Waist48×4 is preserved with exact legacy-emitter geometry controls. Existing fit/cuff thresholds remain unchanged and31 local selected tests passed. Native review against the immediately preceding high-density baseline remains the visual-quality gate; no FPS or timing claim. The next composition issue is still the loose glove-cuff lip, not a reason to inflate the bare band again.

### Layered glove lip reconstruction

Shared support-relative opening policy fits the upper cuff to preceding skin, sleeves or bracers while preserving wrist/hand geometry and unsupported authored shape. Old/Padded/Riding/Fencing retain separate ease and decoration. The unchanged actual lip-gap regression now passes all four glove styles across both sexes/three Centaur builds;42 scoped local tests pass. Fencing supported ease is deliberately3.5 mm, after rejected5/4 mm candidates, not its original unsupported flare radius. Native visual review covers bare, sleeved and band-backed wearers.

An accidentally selected cloak-yoke test failed in untouched code; recorded as observed, not independently proven preexisting. No cloak change or full-suite qualification is claimed.

### Neck band shared attachment reconstruction

Choker and Jewel now use one anatomy-owned elliptical neck section at each band vertex height. A thin radial profile clears the actual neck while preserving band height; Jewel mounts on its own emitted ring, unaffected by other satin accessories. Twelve scoped local tests pass, including actual surface/triangle fit, closed384-triangle geometry, own-mount composition, unchanged Stand collar and head-cache swaps. Stand/torso necklaces and cloak/notch are not changed. Ring cost increases128→384 triangles; no runtime-performance claim.

The isolated-neck candidate was rejected in native review because its legacy low seat remained buried in the torso. A composed-body first-red measured28.319 mm penetration. The corrected seat uses the upper exposed neck station: the ring top ends at that landmark, with the center derived from its half-height. Across Human/Nymph/Medusa, both sexes and three builds, actual head/torso presentation frames give a minimum7.965 mm torso clearance. This is a separate composition witness, not proof against every head pose. Rejected low-seat renders are preserved.

The upper-seat candidate also failed visual review on small male builds: the separate face sculpt contributes its own neck root, beyond the build-scaled neck profile. A new actual-face clearance assertion first failed at0.361 mm against the unchanged1 mm minimum. The next WIP support resolver considers the anatomical face clearance isosurface on the same authored radial ray as the profile, retaining the farther boundary. Hair, jewelry and other accessories never enter that support. Upper-seat rejected captures remain evidence; the corrected support passed the12 scoped tests and independent48-pair native visual gate. Small male rings now remain continuous, gems seated, and Commoner controls stable. Final head-style flag consistency is rerun through the same scoped tests. No universal motion or whole-suite clearance is inferred.

## Reconstructed shared wrapping layout

Torso windings and hanging tails now share first-height, spacing, slope, span and support-clearance parameters. Tail roots select the actual emitted winding instead of duplicating a nominal anchor equation. Partially supported roots fit their complete width to the local connected winding footprint, then return to authored downstream width; fully absent roots remain absent. This corrects the candidate regression that deleted one small-female Commoner tail without changing the authored hem.

The reconstructed slice passed 38 distinct selected tests, strict Clippy and the native build. Independent qualitative review accepted 48 paired views across Human sexes/builds, cloth/plate, standing/walking and sampled strike poses. Existing stiff tails and skirt intersections remain; the cloak-yoke failure and apron shoulder notch are not fixed. This is local sampled qualification, not release-profile/full-suite/hosted qualification, continuous collision clearance, a general disconnected-support solver, or user acceptance. No performance improvement is claimed.

### Shared rounded bands and upper-arm attachment

A shared rounded-band profile owns the axial/radial dimensions and tessellation
of neck bands and armlets. Each caller supplies its support and attachment frame.
Resolution finishes before emission, and missing or non-finite support rejects
an incomplete ring. The neck recipe retains its exposed-neck seat and actual
face support, including its own front-mounted jewel.

Armlets retain their authored upper-arm station and height. Their support is a
bounded local footprint of the already-emitted body, sleeve and armor layers,
including sheer fabric. A maximum over nearby axial and angular samples bridges
an abrupt sleeve or pauldron hem rather than pulling connecting facets through
it. Later robe sleeves still cover the armlet according to existing layer order.
The original ring penetrated a small male Human arm by 24.910 mm in the first-red
regression. The new emitter uses 384 triangles per armlet, compared with 112 in the
fixed-radius torus. This is a correctness/attachment change, not a speed claim.

Eighteen selected local tests, strict Clippy and the native build passed.
Independent review accepted 60 paired Human views across sexes/builds, bare
arms, Hawaiian shirts and plate shoulders in standing/walking poses. Actual
triangle samples over Human/Orc/Nymph arms have 3.412–17.032 mm field clearance;
the unchanged acceptance bounds are 1–18 mm. Neck geometry and its jewel mount
remain within one micrometre of the preceding emitter. This is local sampled
qualification, not full-suite, release-profile, hosted or user acceptance. The sampled footprint is not a
continuous collision certificate for arbitrary disconnected feather/leaf trim.
Cloak-yoke, apron notch and skirt intersections remain separate unresolved work.

### Shared torso necklace attachment

Pendant chains and tooth cords share a closed support-fitted path between the
anatomical head-joint neck seat and the torso core's upper-sternum station.
The actual neck profile, face sculpt and selected torso layers are composed in
the same frame; hair and other ornaments never become support. Recipes retain
separate drops, strand thicknesses, materials and ornaments. Mounts interpolate
the retained chain centerline. The pendant's unchanged setting dimensions follow
the sternum plane. Seven tapered teeth keep their authored lateral splay while
depth fits the chest beneath them. Native plate, shell and equipped armor reach
the same caller. Broad beaded collars and head-joint neck bands remain separate.

The complete path and mounts resolve before emission. Invalid or absent support
rejects the complete necklace. Three samples per connecting span enforce bounded
clearance and relief; refinement permits 64–256 stations. All 72 ordinary body
fixtures use 64 stations, or 768 triangles per closed six-sided chain. This is a
sampled construction-time fit, not continuous collision, arbitrary disconnected
support handling, or a measured performance improvement.

The baseline pendant penetrated the small male Human torso by 5.634 mm. Tests
rejected an omitted broad-body necklace and excessive upright-setting relief.
Native review then rejected a low loop that spread over the shoulders and
turned outer teeth into sideways hooks. Separate neck/sternum anchors and a
fixed lateral tooth rule corrected those issues rather than weakening fit
checks. The final raw/smoothed matrix covers Human, Orc and Nymph, both sexes
and three builds: 72 cases with 1.870–18.858 mm sampled field clearance within
unchanged 1–25 mm limits. Rest-head composition, exact closed seams, translated
landmarks, own-mount independence, missing-support rejection and native-layer
caller witnesses are checked separately. Full-suite, hosted, release-profile
and continuous-motion qualification are not implied. Existing cloak-yoke,
apron notch, skirt and hair overlap problems remain separate work.

The final slice passed 32 distinct selected local tests, strict Clippy, the native
build and 20 documentation checks. Independent review accepted 66 controlled
Human before/after pairs across standing/walking front, side and rear views,
including both sexes, three builds and plated controls. Female rear-neck hair
occlusion limits those views; unobstructed male rear views provide stronger
visible closure evidence. This is local sampled qualification, not user
acceptance, full-suite/release-profile or hosted qualification.

### Coupled Pharaoh collar attachment

The Pharaoh collar resolves as one three-tier assembly around the anatomical
neck and upper-sternum anchors. Its 11 mm tube radius, 24 mm neighboring-tier
centerline spacing, alternating metals/beads and thirteen fringe drops remain
recipe dimensions. The highest tested complete seat is selected within a 12 mm
lower window, in 1 mm steps. The whole profile moves together, including its
front station; individual bands are not shrunk into local neck hollows.

A bounded fixed-chord query in each radial/Y section selects the first exit
from the local support envelope. This permits a tier to rise onto a shoulder
without the horizontal ray jumping to a remote silhouette. Finite route,
connecting-span and spacing checks retain atomic rejection. Fringe roots come
from the retained outer ring, and actual emitted-ring contact is checked. Body,
neck, face and selected torso layers own support; other jewelry and hair do not.
These are sampled construction-time checks, not a general contour or cloth
solver and not a continuous collision certificate.

Each closed six-sided tier uses 64–256 bounded angular stations: 2,304–9,216
triangles across the three bands, compared with 480 in the old nominal rings.
The retained bead dimensions use the caller's raw or softened resolution.
This is a correctness and composition change, not a measured speed improvement.

The final 60-case raw/softened geometry matrix covers Human, Orc, Nymph,
Hobgoblin and Troll, both sexes and all three builds, within unchanged 1–30 mm
sampled field-clearance bounds. Thirty-eight distinct selected local tests,
strict Clippy, the native build and 20 documentation checks passed. Independent
native review covers 66 controlled Human before/after pairs across standing,
walking and sampled strikes, including plated controls. Other species have
geometry evidence only; hair-occluded segments are not visually qualified.
Existing cloak-yoke, apron notch, skirt and hair overlap
remain separate work. Full-suite, release-profile, hosted and user acceptance
are not implied.

### Shared wrist-bracelet attachment

Fine Royal wrist bracelets share the upper-arm ornament's retained-layer support
query and rounded-band profile. Their authored forearm-joint station and 4 mm ×
build axial/radial half-dimensions remain unchanged. The complete local support
footprint resolves before any jewelry is appended; incomplete support rejects
the whole band. The recipe remains fine jewelry, not forearm armor.

Before emission, the complete wrist candidate is sampled against retained layers
for penetration and excessive relief. Crossing a scalloped bell-cuff hem fails
the 18 mm relief budget and omits the complete bracelet without changing prior
layers. Royal plus robe is one such supported refusal, not a solved visible
bracelet-over-robe arrangement. Its remaining garment-composition question is
explicit rather than hidden by a floating ring or changed wrist dimensions.

The caller includes already emitted skin, sleeves, cuffs and metal forearm layers.
Later fur, equipment robe sleeves and gloves keep their existing layer order;
this does not establish clearance against every later garment. The shared rule
retains the qualified armlet parameters and algorithm, and neck bands keep their
separate anatomy/face support. The current retained-layer callers use 24 angular
stations and eight cross-section stations, or 384 triangles per band instead of
the bracelet's previous 240. No performance improvement is claimed.

The final slice passed 51 distinct selected local tests, strict Clippy, the native
build and 20 documentation checks. Across 108 ordinary body/layer cases, actual
triangle samples have 1.795–16.083 mm radial clearance within unchanged 1–18 mm
bounds. Independent review accepted 60 controlled native before/after pairs,
covering three female Human builds, female Nymph, plate/glove/robe controls and
a male Courtier control, with standing, walking and sampled strikes. Hidden
bracelet/glove surfaces are not visually qualified. This is local sampled
qualification, not full-suite, release-profile, hosted, continuous collision or
user acceptance. Existing cloak-yoke, apron notch, skirt and hair issues remain.

### Shared anatomical crown and native winged helm

The face sculpt and open crown use one canonical uncovered humanoid cranium in
the pre-presentation head frame. Its center and radii are unchanged. The crown
profile separately owns directional ease, wall thickness, opening height and
mesh budget; translated/scaled anatomy does not require another outfit offset.
The native winged helm retains a rounded dome and twelve authored feather vanes,
now with a lined, open-bottom shell rather than a solid oversized blob.

The closed band, front setting, feather insertions and upper braid attachment
follow that cap. The braid mount accounts for its actual root-lobe radius.
A shared plait profile separates that fitted insertion from the authored lower
hairstyle endpoint, retaining all eight lobe dimensions and the original lower
lobe/tuft coordinates. Ordinary hanging braids remain geometrically unchanged.
Independent review rejected both an upper braid breakthrough and a subsequent
whole-plait downward shift that worsened inherited cloak overlap. Emitted hair
geometry now supplies separate upper-envelope and retained-tuft regressions.
The existing lower braid/cloak overlap is not solved by this cap attachment.

The 32-around, six-row shell uses 768 triangles, with another 384 for its closed
six-sided rim. This is a construction/fit improvement, not a speed claim.
Canonical crown/rim samples across Human, Nymph, Orc, Hobgoblin, Troll and Centaur,
both sexes and three builds, stay within 1–30 mm field clearance. Actual Human
skin-mesh roof relief is 20.943–21.943 mm, versus the original first failing
witness at 101.072 mm. Other equipment headwear remains unchanged. The native
veiled-cap, tricorn and witch-hat consumers are reconstructed below.

Final local qualification passed 56 distinct selected tests, strict Clippy, the
native build and 20 documentation checks. Independent review accepted 72
controlled pairs across Human and Nymph standing/walking front, side and rear
views, including 24 wider complete-braid/shoulder pairs. The final upper mount
fits and the prior lower endpoint is preserved without visible additional
lower-plait degradation against baseline. Inherited braid/cloak overlap remains
unresolved. This does not claim full-suite, release-profile, hosted, continuous
collision or user acceptance; existing cloak-yoke, apron notch, skirt and other
hair problems remain separate.

### Smooth cap and exposed-hair composition

The Juliet veiled cap now shares the anatomical crown construction, with its own
opening, directional ease and wall. It remains a shallow cap rather than a
replacement hairstyle. The cloth shell and closed trim use one opening; the full
upper veil seam follows its rear arc. Only the upper veil attachment bends.
The original lower veil plane, thickness and four lower corners are retained.
Veil/hair overlap below the cap remains a separate composition problem.

An optional headwear-owned hair envelope applies to this cap, the tricorn and
the witch hat described below, each with its own crown profile.
A monotone height compression and smooth horizontal compression tuck covered
hair beneath the lining. The map remains the identity below its transition;
exposed tails, braids, bobs and other lower landmarks retain their authored
positions. The exposed bun is separately owned: it retains its original mass,
band dimensions and height, while its rear station follows the complete cap/trim
bound with near-contact. It is not crushed into the covered scalp volume.
Face, beard, ears and ornaments are not passed through the hair fit.
Ordinary hair and headwear outside those consumers remain unchanged. The winged helm retains its
separately qualified suppression and two-anchor braid recipe.

Animated loose hair uses the same fit for source surfaces and pinned controls.
Tapered control bounds are measured from complete fitted triangles rather than
assuming that radial compression cannot increase cross-sectional reach.
Only attached radii can expand; free controls, support side and attachment masks
remain authored. This work is prepared with model construction, not fitted every
frame. Hanging-hair cache identity includes the resolved envelope, so bare/capped
swaps cannot reuse mismatched sources while color-only and unrelated clothing
changes still share their immutable hair.

The final source passes 95 distinct selected local tests, strict Clippy, a fresh
native build and 20 documentation checks. Independent review accepted 108
controlled native pairs across all five hairstyles and neutral/walking front,
side and rear views, including 30 wider female hair/veil views. Review rejected
an earlier candidate that crushed the exposed bun despite 93 passing tests;
the repaired separate attachment retains its round silhouette and near-contact
without visible additional lower-veil degradation. This is bounded reconstruction,
not recovered historical source, a full-suite or hosted pass, continuous
collision, a speed result or user acceptance. Existing lower veil/hair,
cloak-yoke, apron-notch and skirt issues remain separate.

### Tricorn crown, opening and hair reconstruction

The native Pirate tricorn now uses the same anatomical lined-crown construction
and headwear-owned hair envelope as the cap, with its own taller crown profile.
A closed annular brim replaces the solid flattened blob across the scalp. Crown,
opening and covered-hair volume share one profile, while the three folded sides
and their original trim paths remain unchanged. The roof stays near its authored
height; this work does not redesign the inherited segmented fold corners or the
projecting brim shelf.

The fit completes at the brim underside, is unchanged below its transition, and
applies to static surfaces, animated loose-hair sources and pinned controls.
Only three upper swept-fringe controls move for the taller/wider tricorn; ordinary
back and side control centers remain authored. Complete affected surface
triangles determine pinned control bounds, with free fall unchanged. Bare,
veiled and tricorn source meshes have separate cache identities, including real
entity swaps in both dressing orders.

A bun is a rigid exposed component, not compressed crown hair. For the broad
tricorn brim it retains its original dimensions and nape x/z station and sits
below the brim, including the tie's full extent. Tests require the complete
style-2 hairstyle to contain exactly one bun, not merely a working helper.

This reconstruction passes 106 distinct selected local tests, including the
previous cap/winged/equipment/hair selections, strict Clippy, a fresh native
build and 20 documentation checks. Independent native review covers 78 controlled
before/after pairs across both Human sexes and all five hairstyles, slim/broad
Human and Nymph long hair, with neutral/walking front, side and rear views.
It is a sampled local checkpoint, not recovered historical source, full-suite or
hosted assurance, continuous collision, a performance result or user acceptance.
Existing lower veil/hair, cloak-yoke, apron-notch and skirt issues remain
separate. The witch-hat consumer is reconstructed below.

### Native witch-hat opening and covered hair

The native Witch hat shares an anatomical opening and conservative covered-hair
volume with the common crown profile. Its brim is a closed annulus rather than
a solid flattened scalp disk. The cone begins at a horizontal mouth seated on
that same opening before continuing through its authored tall backward-bent
path. Its outer width, upper cone stations, bow and sheer tails remain
style features, not a replacement cap. The pink ribbon retains its authored
vertical width and follows actual cone support through the common rounded-band
profile. Its radial section is thin and seated, rather than a torus hidden inside
the rounder cone. Consistent parameter winding keeps the softened ribbon
closed and outward-facing without reversing individual curved faces.
The existing equipment wizard recipe
keeps its established dimensions and geometry.

Covered hair uses the same construction-time map for static surfaces and
animated pinned controls. The conservative interior fit clears the actual cone and ribbon,
not just the nominal brim hole. Hair below the transition is unchanged, while
the exposed bun and tie retain their rigid authored dimensions and nape x/z
station beneath the broad brim. Whole-style tests require exactly one bun.
Bare, veiled, tricorn and witch hair sources have separate cache identities;
free hair controls and their original radii are retained.

Final local qualification passes 133 distinct selected tests, including
prior cap/tricorn/winged/equipment/hair and shared band consumers, strict Clippy,
a fresh native build and 20 documentation checks. Independent native review
accepts 78 controlled before/after pairs across both Human sexes and all five
hairstyles, slim/broad Human and Nymph long hair, with neutral/walking front, side
and rear views. The quiff, short-hair lobe and rear bun breakthroughs are absent,
and the pink ribbon is continuous with its bow still attached.

Below-brim sheer-tail/hair overlap remains a separate composition issue. This
is reconstructed source with selected local evidence, not recovered historical
work, exhaustive or hosted assurance, continuous collision, a performance claim
or user acceptance. Existing lower veil/hair, cloak-yoke, apron-notch and skirt
issues remain separate.

### Shared native shoulder-strap attachment

The Hobgoblin riveted vest and optional Troll harness shoulder straps now share
one anatomy-relative fitted assembly. Chest and shoulder stations come from the
same torso core that builds their wearer; recipes retain their lateral fractions,
lower attachment drop, original 30/32 mm half-widths, colors and support choice.
Vest straps fit garment support; the harness fits bare skin. No species branches
select geometry inside the shared attachment.

Front and back ribbons and the over-shoulder turn use one clearance-grown
surface. Complete terminal rows share positions and normals exactly. Missing
terminal or interior support leaves separate capped strips instead of connecting
an earlier surviving row. The apron now calls the same terminal guard with its
original profile and emission unchanged. The new native ribbon has a 7 mm flat
cross-section instead of the old elliptical tube, preserving width without the
raised arch. Original vest rivets, harness diagonal and ring are unchanged.

Final local qualification passes 35 distinct selected tests, strict Clippy,
a fresh native build and 20 documentation checks. Independent native review
accepts 72 controlled before/after pairs across native Hobgoblin and Troll,
both sexes and three builds, in neutral/walking front, side and rear views.
The raised arches and floating ends are replaced by close-fitting flat straps.
Minor Hobgoblin endpoint highlight faceting remains a cosmetic caveat; inherited
underlayer/body artifacts remain separate. This is sampled local qualification,
not full-suite, release-profile, hosted or user acceptance.
This reconstruction does not solve broad cross-layer composition, moving skirts,
cloak-yoke, apron-notch or below-headwear hair problems. Fitting is prepared with
model construction; no per-frame solve or performance improvement is claimed.

### Reconstructed Centaur chest-wrap boundary

The native Centaur wrap now shares one authored height boundary between its
paint mask and sewn front trim. A reusable boundary profile resolves both
lateral endpoints against the actual support, preserving asymmetric support
extents while retaining the existing symmetric sweetheart style, low edge and
rear height. Interior trim width is unchanged; finite tapered ends replace the
fixed lateral stops. The fitter uses bounded 4 mm support probes and retains
missing rows rather than clamping them. It declines support whose silhouette
is outside that bounded torso search. This is a sampled construction-time fit,
not a proof of continuity through cuts smaller than the probing resolution.

This slice preserves the native male style, underlying torso anatomy, baldric,
belt and other accessories. It does not clip the wrap paint laterally, redesign
the sweetheart edge, repair coarse paint-edge triangulation/underside shading,
or qualify all layers. Local focused geometry and matched native sampled motion
checks qualify this reconstructed slice only, not a full suite, release build,
hosted CI, continuous collision proof or user acceptance.

Final local qualification passes 26 distinct selected Rust tests, strict Clippy,
a fresh native build and 20 documentation checks. Independent native review
accepts 100 matched before/after pairs: the original 60 front/side/rear views plus
40 additional torso-centered left/right three-quarter pairs that correct the
original side camera's partial chest cropping. All ten male control images are
byte-identical. The shorter tapered terminations read as attached turned edges;
stronger seam shadow and inherited jagged paint transitions remain visible.
This is sampled local qualification, not full-suite, release-profile, hosted,
continuous-motion or user acceptance.

### Shared fitted forearm shells and cuffs

Native metal lower arms now follow the same anatomy-owned forearm support as
organic arms. Their authored elbow-to-wrist span, separate elbow pieces and
intentional axial closures remain. Rigid cuffs use a shared finite-thickness
shell: non-bell cuffs fit eased sleeve support, while vambraces fit the actual
preceding metal mesh. Both retain open bores with inner/outer walls and annular
ends rather than closing a disk through the wearer. Bell and draped sleeves
retain their original recipe and geometry.

The shared profile owns span, clearance, thickness and tessellation; callers
supply anatomy or retained layer support. Missing or invalid support rejects the
whole shell before emission. Neighboring axial/angular samples and polygon
inradius compensation bound ordinary limb facets. This is a sampled
construction-time fit, not arbitrary disconnected-obstacle or continuous
collision qualification. The existing attachment frame and bracelet/later
outerwear/glove ordering are unchanged; no per-frame fitting is added.

The new geometry has an explicit budget: 320 triangles per base metal forearm,
448 per metal vambrace and 576 per non-bell cuff. This correctness/attachment
work is not a performance improvement claim. Later equipment, cloak/armor
clearance and moving skirts remain separate composition concerns.

Final local qualification passes 38 distinct selected Rust tests, strict Clippy,
a fresh native build, formatting and 20 documentation checks. Independent native
review accepts 236 controlled before/after pairs across 58 case sets, including
Human/Orc Knight, Human/Nymph Ranger, mirrored wrist views and medium native
Orc/Nymph/Hobgoblin/Troll bare arms. All 12 Pirate bell-control pairs are exactly
pixel-identical. The closer wrist taper and smoother cuff/forearm joins preserve
armor and leather styling. Some neutral female wrists and rear Ranger cuffs
remain occluded, so hidden surfaces are not qualified by those views.

This is reconstructed source with sampled local qualification, not recovery of
historical work, a full-suite/release-profile/hosted pass, continuous-motion
collision, universal bare/sleeved layer composition, performance or user
acceptance. Existing skirt, cloak and torso/belt artifacts remain separate.

### Shared finite pouch attachment strips

Pouch attachment strips now use a reusable radial patch profile with explicit
height/angular span, support clearance, leather thickness and tessellation.
Their two faces, side edges and terminal caps form a closed oriented mesh rather
than one-sided sheets. Recipes retain the previous two strap stations, widths,
bag, flap and tint. Each strip resolves the composed clothed torso and pelvis in
the hip frame before emission; missing or invalid sampled support rejects that
strip without emitting a partial mesh. Neighboring half-cell probes preserve
sampled holes, not arbitrary sub-resolution obstacles or curved-surface collision
proofs. Fitting is construction-time, with no added per-frame solve or draw call.

The explicit geometry budget is 104 triangles per strip, replacing 16, for 176
additional triangles per pouch. This is a sewn attachment-strip reconstruction,
not a modeled belt pass-through loop, universal cross-layer composition or a
performance improvement claim. Native torso/belt seams, skirts and later layers
remain separate concerns.

Final local qualification passes 25 distinct selected Rust tests, strict Clippy,
a fresh native build, formatting and 20 documentation checks. Independent native
review accepts 144 controlled before/after pairs across 24 waist-centered case
sets: Human/Orc/Nymph Ranger sex/build variants, native Human and Scout, Tavern,
Rogue and Knight controls. All 12 no-pouch Knight pairs are pixel-identical.
The unchanged bag/flap prefix is verified by a same-VM emitted-vertex diagnostic
and preserved source; the portable test does not assume platform-specific
floating-point bits.

Strap edge thickness is visible locally, while inherited belt/torso slits and
arm/garment occlusion remain. Hidden strap portions are not qualified by those
views. This is bounded reconstruction with sampled local evidence, not recovered
historical source, exhaustive/release-profile/hosted qualification, continuous
collision, general layer resolution, a performance claim or user acceptance.

### Native forearm recipe selection before construction

A small typed forearm plan now resolves immediately after outfit/equipment
composition, before fitted support geometry is created. Both forearm occurrences
use that plan in their existing joint frames. It selects the bare, eased-sleeve,
metal or specialized body surface and the native cuff once, preserving styled
bell/drape, robe and ordinary-cuff precedence. Cuff variants explicitly distinguish
an authored bell, an anatomy-owned eased-sleeve fit and a retained-metal fit.
The construction path consumes those choices instead of selecting clothing again.

The initial preservation checkpoint kept the sequence explicit: bracelet over
the base/native cuff, then fur, equipment sleeve/wrapping and glove. Existing fit helpers, colors, material
buckets, attachment frames, caches and whole-piece rejection are unchanged. A
planned bracelet is a fitting request, not a claim that its placement succeeds.

This bounded recipe is not an occlusion solver or a generic layer graph. Open
cuff ends, sheer drapes and scallops do not prove complete hiding, so every
previously requested piece remains. No fixed forearm subregions, coverage
registry, automatic culling or hidden-support retention mechanism is introduced.
The existing containing-solid detail mask remains separate; no performance
improvement or universal composition is claimed.

Local qualification passes 51 selected Rust tests, strict Clippy, a fresh native
build, formatting and 20 documentation checks. A frozen pre-plan differential
compares every Builder field in all material buckets for 20 composed forearm
cases, including both sides. Native review covers 90 before/after views across
15 fixtures in neutral and walking poses. Initially 87 pairs are pixel-identical;
the other three differ at 24 pixels total. Same-executable repeat captures reproduce
those exact elbow, knee and heel regions, and independent review finds no
perceptible forearm regression. This is sampled local preservation evidence,
not universal pixel determinism, a full-suite/release-profile/hosted pass,
continuous collision proof or user acceptance.

### Explicit bracelet support beneath a loose equipment robe

The forearm plan now chooses between the existing base-plus-cuff bracelet support
and the bare forearm beneath a loose equipment robe. The latter applies only to
a bare base with a robe-generated native cuff and actual equipment Robe. Styled
bells, drapes, sheer sleeves, plate and fur retain their previous policy. Fitting
uses the existing complete-ring resolver against a read-only support reference;
the result is appended at the existing ornament stage. Missing selected support
still rejects the whole bracelet, without borrowing a later garment as fallback.

This separates support from visibility: a correctly supported bracelet under the
robe may be hidden, or visible through an opening in some poses. It does not put
jewelry over the outer robe, declare any region universally occluded, introduce a
coverage graph, or remove hidden geometry. The preceding behavior-preserving checkpoint remains
retained separately.

Local qualification passes 55 selected Rust tests, strict Clippy, formatting,
a fresh native build and 20 documentation checks. The intended first-red test
observed zero bracelet triangles in the prior production robe recipe; the new
recipe retains a complete 384-triangle ring. Thirty-six composed forearm cases
cover female Human/Orc/Nymph builds 0–2, both sides and both smooth settings.
They check actual bare-skin clearance, complete-ring topology, conservative
whole-triangle separation from native cuff and equipment sleeve, and exact
non-bracelet geometry and materials. Sixteen Human glove cases additionally
cover all four styles, builds 0/2 and both sides, including unchanged later metal
buckles. The frozen recipe comparison keeps 18 cases wholly identical and two
intentional robe cases identical apart from the added ring.

Independent native review accepts 121 paired views: 90 composed-outfit forearm
views, 16 opening close-ups and 15 complete-outfit overviews. Band portions are
visible inside some bare-robe openings and hidden in others; sampled glove
views remain unchanged. Of the original pairs, 84 are pixel-identical, 36 have
intended band/local shading changes and one no-robe control differs by one pixel.
A repeat from the unchanged baseline executable reproduces that pixel state;
all six repeated baseline/candidate control views then match exactly. The
original discrepancy remains recorded, not reclassified as exact. No new
robe/cuff silhouette change is observed. This is bounded local evidence, not
universal visibility, continuous whole-actor collision, full-suite/release/hosted
qualification, a performance claim or user acceptance.

### Shared native torso foundation

The next cross-layer review found no benefit in extending the forearm recipe
into a generic torso-region solver. Apron draping still needs the separately
deferred local transition over the baldric; declaring a layer order does not
supply that transition or prove coverage by thin cloth. The rejected apron
integration remains disabled, and the existing containing-solid armor proof
remains restricted to its supported consumers.

Instead, one small shared foundation now owns the native torso/bodice support
choice previously repeated by torso emission, baldric fitting and outerwear
fitting. Body emission keeps its own foundation; fitted outer pieces select the
existing equipment shirt when present. Baldric native-plate precedence and
outerwear's ordered union of all retained shells remain distinct. No new plan
type, cache, graph, coverage policy or geometry change is introduced.

Local source qualification passes 51 selected Rust tests, strict all-target
Clippy, formatting and fresh test/native builds. An exact ordered SDF-operation
comparison covers 108 foundation cases, including both sexes, three builds,
shirt presence, neckline trim and all native bodice support choices. Eight
additional layered cases preserve the distinct outerwear and baldric support
recipes and every emitted baldric Builder field. Those cases deliberately
include simultaneous shell, native plate and equipped armor. Existing apron,
shirt, wrap-tail and baldric owners remain green. This is behavior-preserving
refactoring, not an apron fit fix or a measured performance improvement.

Independent native review covers 72 complete-outfit before/after views across
12 fixtures, with front/side/rear neutral and walking poses. Sixty-nine original
pairs are pixel-identical; three differ at 51 pixels total near boot edges and
a skirt/hand boundary. Unchanged-executable repeat captures reproduce all three
alternate states, including a second Commoner baseline run matching all six
original candidate frames. The original discrepancies remain recorded, not
reclassified as exact. No perceptible outfit or torso regression is observed.
Twenty documentation checks also pass. This is bounded local preservation
evidence, not universal pixel determinism, full-suite/release/hosted
qualification, continuous collision proof or user acceptance.

### Reconstructed support-fitted thigh garter

The thigh tie now reuses the existing rounded-band fitter against the actual
preceding thigh layer, rather than a guessed radius. The completed ring owns
both the knot mount and the tail-root mount. Each is resolved before any part
is appended; missing band support rejects the complete assembly. The tail's
radial thickness frame places its inward root edge on the ring at the actual
root height, not at the band's wider mid-height.

The authored seat, band height and radial thickness, knot dimensions, asymmetric
tail outline, colors and left-thigh/Guardian selection are retained. The band
uses 384 triangles instead of 112; the knot and tail recipes keep their existing
budgets. No new plan type, layer graph, per-frame fitting or cloth simulation is
introduced. The free-hanging tail is still a rigid authored piece.

The intended first-red witness measured 41.25 mm of penetration by the old
complete tie into a wide female Human thigh. The unchanged regression now
passes. Local qualification passes 42 selected Rust tests, strict all-target
Clippy, formatting and fresh test/native builds. A 108-case matrix covers
Human/Orc/Nymph, both sexes, three builds, both shading modes and actual bare,
trouser and plate thigh support. It checks seven samples per accessory triangle,
actual emitted knot/ring intersection, tail-root contact/frame and asymmetric
tip positions, with exact retained geometry/material-prefix preservation.
Separate witnesses cover whole-ring closure, dimensions, all six support
material buckets, transformed frames and atomic missing-support rejection.
These are bounded sampled geometry checks, not continuous collision proofs or
arbitrary disconnected-layer qualification. Later skirts and fur remain separate
composition concerns; no performance or full-suite/release/hosted claim is made.

Independent native review accepts 104 before/after pairs: 72 complete-outfit
views and 32 thigh close-ups across 12 fixtures. All 18 no-garter control pairs
are pixel-identical. The restored visible band and connected tie improve the
previously buried attachment, with no significant whole-outfit regression in
the inspected neutral and walking frames. One minor new limitation is explicit:
the Rogue neutral side close-up shows a small knot protrusion through its later
red skirt. This does not materially disturb the full-outfit silhouette, but
later-skirt clearance is not solved. Twenty documentation checks pass.

The follow-up actual-mesh investigation establishes that the Rogue limitation
also includes the ring, not only the visible knot. The complete-garter witness
reports 94 intersecting garter/skirt-or-trim triangle pairs at the sampled
neutral pose, including a face inside the 384-face ring. A proposed front-facing
knot would preserve those ring intersections and was rejected before production
integration. The supported-ring checkpoint is retained; cross-joint skirt
composition remains unresolved. Fixing that requires an explicit support and
motion policy, not a cosmetic knot move or acceptance based on viewing distance.

### Crossed-waist bow: support-following upper-cord placement

The November 11 VM continuation resolves the tie against the actual emitted upper
cord, leaving all three cord recipes unchanged. The bow retains its planar pattern
and depth thickness while following the existing waist support. Its hanging ends
are 6 cm long and stop above the separate belt/buckle. A tiny central stitch stays
on the cord; the rest reserves the lobe half-thickness and a support margin through
existing fitted-append normal handling. No general attachment or layer solver was
introduced, and other waist recipes are unchanged.

The first lower-cord body-fit candidate passed numerical body clearance but was
rejected visually for buckle penetration. The upper-cord cut avoids that conflict
through authored placement rather than querying or deforming the belt. Contact,
shape/thickness, unchanged-cord and 1 mm sampled support-clearance tests pass across
108 model/wardrobe/sex/build/detail/frame cases. Thirty related regression tests and
strict all-target Clippy pass. The 202-frame capture set covers 18 crossed-cord
configurations, three controls and extra detail views. Front/side neutral/walking
visual review accepts the bow, with representative whole-outfit checks; this is
sampled acceptance, not exhaustive animation or overall outfit qualification.
Known skirt/leg intersections and Goddess waist gaps remain separate work.

### Goddess waist overlap: existing supported seam

The November 12 continuation sets the Goddess skirt's existing `waistline` field
to 7 cm. Both outer and sheer skirt layers inherit it, so their rim overlaps the
bodice on composed waist support instead of opening below it at the generic 4 cm
pelvis ring. No new fitting primitive, attachment registry or animation system
was needed. A 9 cm candidate was rejected in favor of 7 cm because its lip hid the
lowest cord; the final cut keeps that loop and the qualified bow readable.

Actual emitted-rim tests cover both layers on Human, Orc and Nymph across three
builds. All 58 wardrobe tests and four bow tests pass, as does strict Clippy.
Native review accepts the broad-gap correction across nine Goddess configurations
in sampled neutral/walking front, side and rear views. All 120 requested frames
were captured, including three unaffected outfit controls and tie close-ups.
Small dark side pockets, a fine join line and the rounder upper-skirt transition
remain close-up polish work. Walking thigh breakthrough remains a separate open
problem; this change does not claim general skirt/leg collision correctness.

### Harness ring: retained ribbon support

The November 12 harness continuation keeps the existing diagonal and shoulder
ribbons intact, while attaching the rigid ring to the resolved front strap.
The ring frame follows a chord between two retained centerline supports, then a
bounded sampled footprint supplies an outward depth-only clearance. This reuses
`FittedStrap` and Builder triangle access in the local ornament recipe; there is
no species branch, new attachment registry, cloth solver or animation change.
An absent supporting ribbon no longer produces floating hardware.

Independent emitted-cloth rays pass across Troll, Orc and transferred Human,
both sexes and three builds (18 fixtures). Denser acceptance samples preserve
strict non-penetration and keep both attachment-region minima between 0.44 and
1.86 mm. Pairwise mesh distances verify the ring remains rigid with unchanged
dimensions and topology, and the complete original ribbon recipe is exact.
Eighty-eight focused tests, strict all-target Clippy, formatting, architecture
and documentation checks pass. Native review accepts all 12 Troll/Orc
configurations in neutral/walking front/side join views, plus representative
whole outfits, with 96 final frames. Female Troll rings now read as complete
closed hardware rather than partly buried arcs. These are two captured instants,
not a continuous gait or complete orbit.

Walking skirt/leg contact is still open. The existing actual-triangle diagnostic
passes neutral poses but fails walking/strike for native short skirts and
chain-mail tassets; that strict failure is retained and disclosed. Short skirts
currently rotate as rigid drapes rather than consuming posed-leg support.
Blanket widening or reduced stride was not substituted for a contact solution.
Lifted shoulder tabs, lower harness-strap clearance and the jagged Orc neckline
remain separate polish work. No full-suite, release, hosted-CI or performance
qualification is claimed for this checkpoint.

### Native belt ring ownership and attachment (2026-11-12)

The native loincloth recipe now supplies the belt's hardware instead of also
receiving the default rectangular buckle. The satin belt band remains exact,
and all other hip-decoration recipes keep the original buckle. The rigid ring
joins the original panel's upper contact to the fitted waist with its unchanged
7 cm centerline diameter and 1 cm tube radius. A bounded static station solve
sets position/orientation once; there is no new animation system or per-frame
mesh rebuild. Unsupported spans retain the original mounting with a warning.

The independent six-body Orc matrix checks actual emitted panel/waist contact,
visibility against waist, skirt and peplum meshes, and exact original cloth,
stripe, normals, colors and topology. Upper ring visibility is 99.3–100% in the
sampled front rays, compared with the rejected overlapping-buckle candidate's
incomplete arc. Belt-band and default-buckle recipe tests cover Orc, Troll and
Human supports. Native front/side review must still judge the ring as hardware,
not merely accept some near-contact rays.

The original panel clipping is not fixed. A rigid-panel clearance experiment
removed standing holes but detached the mount and was rejected. That source,
strict tests and captures are retained separately. A second rejected ring
candidate exposed the duplicated buckle. Standing panel contacts and existing
walking short-skirt/tasset contacts remain explicit failing diagnostics, not
weakened clearance thresholds. The earlier radial skirt deformation experiment
was also rejected for shape, upper-waist contacts and frame cost; it is not in
production source. No full-suite or continuous-motion qualification is claimed.

Final native review accepts the scoped hardware correction across six Orc
sex/build configurations, neutral tick 0 and one walking sample at tick 18,
with 72 final frames. Both Troll controls are pixel-identical in all 16 frames.
The ring reads as a complete tilted ellipse, without the prior gold-block
occlusion. Ninety-three focused tests, strict all-target Clippy, formatting,
architecture and 20 documentation tests pass. Existing panel holes remain
visible and unchanged; this is not a panel-contact or full-gait fix.

### Support-fitted native loincloth panels (2026-11-12)

The next bounded candidate replaces the flat panel depth with a static, row-wise
fold derived from the actual resolved skirt and peplum emitters. It clips support
triangles to the complete original closed shell, builds a short concave depth
profile, and subdivides the existing extrusion along that profile. Front and
rear are fitted separately. The original projected outline, hem, paired shell
thickness, stripe alignment and pigments are preserved. The existing fixed-size
ring fit follows the corrected upper cloth contact; belt hardware ownership and
the satin band are unchanged. A rejected ring bracket omits the unsupported ring
with a warning, and the strict ring-presence gate exposes that failure.

Only cold accessory cache misses reconstruct support triangles and fit geometry.
Warm instances share the same mesh handles and unchanged payloads. There is no
new per-frame solver, collision registry or dependency. Native panel/stripe
geometry uses 192–252 triangles across six Orc bodies, up from 72; the ring stays
at 112 triangles. This is a bounded geometry count, not a measured frame-time
claim.

The strict standing actual-triangle and dense-ray checks now pass all six native
Orc sex/build fixtures. Fixed ring dimensions, actual belt/panel contact and at
least 90% visibility remain mandatory. The independent authored-shape check
caught a numerical root-station defect, fixed by conservative coalescing rather
than weakening shell-thickness tolerances. The positive checkpoint contains 102
focused tests, including prior bow, harness, wardrobe and garment coverage.
Native comparison and final documentation checks are recorded separately before
promotion. This static fit does not claim to solve walking skirt/thigh contacts,
continuous motion, all equipment combinations or hosted CI.

Final independent review accepts all 72 matched Orc images: the front/rear holes
are gone, roots and ring remain connected, and side profiles show modest folds
without a visible regression in the sampled walk. All 16 Troll controls are
pixel-identical. Strict all-target Clippy, formatting, architecture and all 20
documentation tests pass. The existing walking skirt/thigh diagnostic retains
exactly 167, 373, 227, 162, 144 and 434 crossing pairs and remains open.

### Buried anatomical neck root (2026-11-12)

The Hobgoblin's gray bib spots were head-owned neck skin crossing the torso,
not missing cloth or an intended neckline. Bind-pose triangles remained hidden;
actual native idle head rotation advanced the lower root by about 6 mm. Exact
captured-pose tests reproduce the spots on female builds 1 and 2 at tick 2, with
0.94 mm and 2.39 mm maximum breakthroughs. The corresponding walk capture is
tick 18. These neck/shoulder scripts use idle tick 2; the accepted panel's own
Orc scripts use tick 0. Neither is a continuous-motion review.

A small anatomy-owned helper fits only the lowest neck ellipse inward against
existing torso support, accounting for the distinct head and torso presentation
scales. Its 8 mm buried-overlap allowance covers the observed ordinary head
movement plus a sampling allowance; it is not an arbitrary-pose guarantee.
The three upper stations, attachment profiles, centers, triangle count, cache
identity and generic legacy neck recipe stay unchanged. Already deeply buried
roots remain exact. The fit runs once at head mesh construction, using 32 radial
support queries; no pose solver, dependency or per-frame work is introduced.

Five focused regression tests cover exact captured idle/walk transforms and
actual emitted-triangle occlusion, upper neck and band preservation, continuous
noncollapsed root geometry, anatomy transfer and open-neck outfit independence.
Native comparison and broader regressions remain required before acceptance.
The rejected shoulder-tab seating experiment is not included in this source.

Final independent native review accepts all 96 candidate frames: the female
Hobgoblin bib spots are removed, with no new visible neck pinch, hole or seam.
Human, Nymph and Medusa open-neck controls remain continuous, though female
lower-neck shading changes slightly. Forty-nine frames are pixel-identical;
47 differ, including a few tiny full-walk floor-shadow changes. This is visual
acceptance of two sampled poses, not a claim of universal pixel identity.
All 125 distinct focused tests, strict all-target Clippy, formatting,
architecture and 20 documentation tests pass. The separate walking skirt/thigh
diagnostic retains its original six failure counts and remains open.

### Smooth vest shoulder support (2026-11-12)

The male Hobgoblin's jagged strap highlight came from fitting directly over the
hard union of the torso and clipped leather bib. RivetedVest shoulders now use
a conservative smooth layer union, separate from the unchanged actual garment
support used by rivets. It uses the current Figure's inner torso and outer layer;
other ornaments, Troll harness straps, clothing meshes and the accepted buried
neck root remain unchanged. There is no new per-frame solver or subdivision.

The blend applies wherever layer fields differ by less than three ribbon
thicknesses (21 mm), not just on the literal bib edge. Its maximum scalar-field
expansion is 5.25 mm, not a vertex-motion bound. Measured matched strap vertices
move 19.15–22.16 mm around the bib transition, including underside normal-offset
rotation. Below y=0.42, maximum changes are 0.687–2.432 mm. Upper y>0.47 changes
are 0.237–0.343 mm; all verified inactive-field samples stay within 0.458 mm.
Exact upper vertex identity is not claimed: approximate ray intersections can
change when earlier marching steps change, even for identical final fields.
The preservation test uses the existing 1 mm geometric error allowance.

Four new regression tests cover the original noncontracting normal jump,
conservative complete emitted-strap clearance across 18 body fixtures, exact
missing-layer behavior, and actual torso routing with unchanged rivets and
bounded inactive-region geometry. All 14 shoulder and three torso-foundation
tests pass with strict all-target Clippy. Triangle counts remain unchanged.
Sampled complete-shell hard-support field clearances are 6.17–19.81 mm;
these are not universal physical-distance or animation guarantees.

Independent native review accepts 96 matched before/after views across six
Hobgoblin and six Troll configurations, idle tick 2 and walking tick 18. The
jagged bib-edge highlight is replaced by a smooth transition, without new visible
bulges or transverse creases. Forty-seven of 48 Troll control frames are exactly
identical; the remaining comparison has seven changed lower-garment pixels.
Existing lower tab stand-off and walking skirt/thigh diagnostics remain open.
This does not qualify continuous gait, every outfit, release performance or CI.


### Reconstructed sleeve-fold candidate (2026-11-12)

The lost sleeve trial has been reconstructed from the retained source, not
recovered as a previously qualified change. Individual capsule segments formerly
blended into the sleeve independently, producing beads at their overlapping
endpoints. The candidate builds each complete fold with a tiny internal blend,
then joins it to the sleeve once. Local support-ray refinement keeps the shallow
fold seated accurately without changing the general attachment helper. Fold
heights, tapered ends and outer-elbow policy remain unchanged; no new solver or
per-frame work is introduced.

Two new numerical regressions reproduce the original 4.75 mm crest swelling per
unit build and now pass the preserved 3.5–4.4 mm relief range and outer-elbow
checks. Existing shared-arm source-field/triangle-budget and resolved female Mage
forearm budget tests also pass unchanged. The full focused surface group passes
11 tests, in addition to the two new gathers tests. Strict Clippy, formatting,
architecture and 20 documentation checks pass locally.

Native before/after review covers 36 matched views across male and female
Commoner, Ranger and Mage fixtures at rest and one walking sample. No new sleeve
holes, clipping or silhouette regressions were seen. The improvement is subtle:
sharp elbow creases and upper/forearm seams remain, as do unrelated walking
tunic/thigh contacts and Ranger waist/strap discontinuities. This is a bounded
geometry cleanup, not a claim that sleeve appearance is finished. Neither
continuous motion, all outfits, runtime performance nor hosted CI is qualified.

### Softer shared sleeve folds (2026-11-12)

A follow-up keeps the complete-fold construction but seats its path 5 mm into
support and blends it over 12 mm per build. This spreads the flank without
flattening the crest. The new first-red found only 0.687 mm relief at a 7 mm
transverse offset against a 4 mm crest. Its unchanged contract now passes:
at least half crest height at 7 mm, no raised pad at 25 mm, and the existing
3.5–4.4 mm crest range. All three gathers and eleven surface tests pass, including
unchanged mesh budgets. Strict Clippy and formatting pass locally.

Thirty-six matched native views across Commoner, Ranger and Mage bodies at rest
and one walking sample show a meaningful local improvement: cut-like jagged
creases become rounded cloth folds without an inflated elbow silhouette. The
upper/forearm join remains visible; this is not a seam or continuous-motion fix.
Existing tunic/thigh intersections and Ranger strap gaps remain. No new solver,
per-item branch or per-frame processing was added. Hosted CI and performance
remain unqualified.


### Supported panel clearance trial (2026-11-12)

A separate bounded trial starts from the qualified soft-sleeve checkpoint, not
the rejected waist candidate. Sewn panels previously added only a Z offset,
which left some emitted tabard triangles intersecting the curved cuirass. The
shared panel helper now trials the existing offset-surface ray at its original
clipped X/Y coordinates. Unsupported rays keep their old fallback; the input
support and panel grid are not expanded or mutated. No new solver is introduced.

The corrected emitted-cuirass regression reproduces 10 failures on qualified
source after distinguishing intentional buckle occlusion from underlying plate.
Three focused controls and all 18 garment tests pass locally, including original
footprint/miss behavior and emblem/baldric layer ordering. Front and back cuirass
tabards and native caster plackets use this helper, so visual controls must
cover those callers rather than only the original female Knight witness.
Strict Clippy, formatting, architecture and 20 documentation checks pass locally.
All 64 matched native pairs across male/female Knight, Sentinel, Mage and Red
Mage at seed 7/default build were reviewed. The female Knight gold-edge hole
closes at both sampled ticks without new floating or shelves; emblems, straps
and rear panels remain intact, and caster plackets stay stable. This bounded
visual pass does not qualify other builds, continuous motion or the rejected
waist work. Performance and final checkpoint qualification remain pending;
no hosted-CI result is claimed.


**Performance gate, not final acceptance:** The local cold-construction
comparison recorded median model time 529.289 ms baseline versus 660.683 ms
candidate (+24.8%); torso-parts time rose from 243.244 to 301.675 ms. Mesh,
triangle and vertex counts were identical. These are measured construction
costs, not per-frame timings. The visual improvement is retained as WIP only:
optimization and fresh qualification are required before promotion.


### Panel query reuse, qualification pending (2026-11-12)

The follow-up retains whether the original clipped projection found support,
instead of tracing that same support a second time merely to recover the answer.
The public point-projection fallback and clipping policy are unchanged; the
panel still uses the existing offset ray only on supported points. A frozen
three-query reference verifies exact emitted triangle positions and colors for
front/back supported, laterally clipped and fully unsupported fixtures at all
three caller offsets. All 19 garment tests and strict Clippy pass locally.

The earlier 64-pair visual review belongs to the pre-optimization candidate.
Its reuse is supported by the exact-geometry regression; planned fresh
representative renders are a separate check, not another fresh 64-pair review.
The previously recorded +24.8% construction slowdown remains the latest
performance evidence until the optimized measurement completes. This follow-up
is still WIP, not performance-qualified or promoted.


### Bounded panel fix accepted locally with construction overhead (2026-11-12)

The optimized follow-up passes 19 garment tests, strict Clippy, formatting and
architecture checks. Twenty-four fresh views cover female Knight, Sentinel and
Mage at eight views each; the hole correction, layering and stand-off remain
intact. The earlier 64-pair review is reused with the exact-geometry regression,
not represented as 64 newly rendered comparisons.

A fresh paired construction run measured total medians 510.359 ms baseline and
560.423 ms optimized (+9.81%), with overlapping ranges 478.947–727.260 ms and
517.160–688.940 ms. Torso-parts medians were 230.684 and 254.963 ms, a 24.279 ms
increase (+10.52%). Counts remain 190578 generated triangles, 105520 vertices,
28 meshes and 16 cache misses. This is a known construction-cost tradeoff, not
performance neutrality; the earlier unoptimized +24.8% run remains historical
evidence, not the latest measurement or a directly comparable speedup claim.

The bounded visual fix is accepted locally with that disclosed overhead. There
is no per-frame algorithm change and no FPS claim. Other builds, continuous
motion and hosted CI remain unqualified; rejected waist work is not included.


### Shared panel endpoint-row reuse (2026-11-12)

Adjacent profile windows now reuse their common evaluated endpoint row. The
local reuse preserves clipping, offset semantics, topology and emission order;
there is no global cache. The frozen exact-output regression now covers eight
profiles, both sides and three offsets, including descending and repeated
heights, signed-zero endpoints and supported/clipped/missing transitions.
All 19 garment tests, strict Clippy, formatting and architecture checks pass.
Sixteen fresh paired views preserve panel trim and hardware; existing Mage
waist gaps and Knight neckline issues remain outside this change.

Deterministic fitted-point evaluations fall from 3554 to 3254 for female Knight
(8.44% fewer) and 5224 to 4876 for male Knight (6.66% fewer). These are point
evaluations, not SDF-query counts. The paired timing result is inconclusive:
model medians 535.867 to 531.388 ms (-0.84%), torso-recipe medians 247.180 to
251.344 ms (+1.68%), with large outliers. Counts remain 190578 generated
triangles, 105520 unique vertices, 28 meshes and 16 cache misses.

Accepted locally as a small deterministic removal of redundant work, not a
measured speedup. This does not erase the earlier disclosed 9.81% construction
overhead relative to the original panel recipe, and makes no FPS or hosted-CI
claim. Earlier broad visual evidence remains reused, not newly rerendered.


### Shared shell-supported native helmet rim trial (2026-11-12)

The native Helm rim now trials the existing rounded-band profile fitted to
the shell alone. Shell shape, visor and plume retain their original recipes;
this is a reusable support-based attachment improvement, not a new helmet
framework or per-item exception. The original upper rim penetrated the shell
by 1.6547 mm. After two intermediate clearance failures, 4480 emitted-triangle
samples retain 1.096145–13.000017 mm relief within unchanged 0.9–13.1 mm bounds.
Eight head tests, strict Clippy and native build pass locally.

Thirty-six matched native views across six fixtures show the rim seated around
the front, sides and rear, eliminating the sawtooth crossing without a floating
gap or broad shelf. Other helmet details remain stable; the Knight breastplate
neckline remains outside scope. The rounder rim costs 496 additional triangles
(144 to 640). This bounded visual result awaits performance and final checkpoint
qualification; it does not qualify all helms or continuous motion.


**Local acceptance with disclosed cost:** All 12 benchmark jobs completed with
verified provenance. Head-recipe medians rise from 44.543 to 51.704 ms
(+7.161 ms, +16.08%); head-total medians from 64.101 to 72.146 ms, and head-finish
medians from 19.596 to 20.073 ms. Overall construction medians were 548.676 and
605.282 ms, a noisy aggregate not attributable solely to the rim. Generated
triangles increase from 190578 to 191074 (+496) and unique vertices from 105520
to 105768 (+248); 28 meshes and 16 cache misses remain unchanged.

The bounded native Helm fix is accepted locally for its clear visual benefit
with those construction and geometry costs disclosed. There is no per-frame
algorithm change and no FPS claim. All-helm coverage, continuous motion and
hosted CI remain unqualified.


### Parametric native cuirass neckline binding (2026-11-12)

The Knight binding now follows the actual intersection of the existing tapered
opening and uncut torso support rather than its flat authored V. Both sides
resolve against their own body profile, then share one continuous strip across
the bottom. The existing fitted-ribbon emitter retains the front support for
this narrow edge; it no longer projects through the opening toward the rear.
The cuirass cut, body, helmet, palette and other attachments remain unchanged.
This is a bounded parametric support operation, not triangle collision or a
new general composition solver.

The new emitted-mesh regression fails on the original small Human fixture:
a binding vertex at z=0.005212 lies behind its front support at z=0.131850.
The retained final-spawn diagnostic also measured a 6.450 mm exposed steel
strip at y=0.410; its pre-spawn counterpart was 5.864 mm. These are witnesses,
not a global minimum-clearance proof. The first contour candidate omitted the
binding because the support already has a neck aperture and was rejected.
The next candidate's separate arms left a visible bottom slit; the continuous
strip removes it. A Centaur test fixture was corrected to Sentinel because
Centaur uses a separate quadruped torso recipe; no Centaur fix is claimed.

Twenty garment tests, strict Clippy, formatting, architecture and 20 documentation
tests pass locally. The new regression covers female Human/Sentinel small,
default and broad builds. Twelve matched baseline/candidate views (24 native
PNGs) at rest and one walking sample show the scalloped exposed steel and bottom
slit removed, while retaining the body/outfit distinctions. This is bounded
visual acceptance, not all bodies, all outfits or continuous-motion coverage.

Eight sequential construction-profile runs include one warmup pair and three
measured pairs in alternating order on the same VM. Total-construction medians
are 584.643 and 601.636 ms; torso-recipe medians are 278.645 and 272.866 ms,
with ranges 265.644–281.149 and 271.699–297.264 ms. The noisy small sample does
not establish a speedup. Generated triangles rise 191074 to 191750 (+676),
unique vertices 105768 to 106044 (+276); 28 meshes and 16 cache misses remain.
Accepted locally for the clear visual benefit with those costs disclosed.
No per-frame algorithm changed; no FPS, hosted-CI, publication or merge claim.


### Contour-query optimization rejected; parameter coverage retained (2026-11-12)

A proposed work reduction reused the last sampled crossing interval and stopped
its refinement at two micrometres. A new comparison against the retained
full-width reference found a 0.754 mm contour displacement on broad build,
bust profile 0 at y=0.456228. The support query can change branches near an
aperture; the narrower interval was not equivalent to the existing refinement.
The production change was rejected and the qualified fitter restored. This was
not a demand for pixel identity: a small query optimization did not justify an
unreviewed contour-branch change or extra solver complexity. No speedup is claimed.

The added contract is retained across nine body profiles (three builds by three
bust profiles), both sides and 21 contour rows per side. It verifies finite,
ordered complete profiles, sampled cutter support and a five-micrometre bound
against the previously accepted refinement. Native front-support and broader
garment contracts remain unchanged. This expands parameter regression coverage;
it is not new visual qualification for every bust profile.

A separate read-only sleeve review inspected 18 fresh native views of male and
female Commoner, Ranger and Mage at rest and one walking sample. The residual
elbow line crosses independently rotated, nonuniformly scaled joint meshes.
No new open hole was established. A size-only or normal-only patch would not
establish continuous cloth fit, so no skinning rewrite, normal override or seam
fix was introduced. Existing accepted folds and neckline geometry are retained.


### Seated lower vest attachment tabs (2026-11-12)

A native side-view review reproduced visible lower shoulder-tab stand-off.
The first-red emitted-mesh witness measured 7.163 mm underside field clearance
on the slim male Hobgoblin. Uniform 14 mm support inflation reserves the
7 mm ribbon shell but also leaves its sewn ends about 7 mm off the garment.

The bounded candidate keeps the existing upper shoulder fit and bridge. Only
the lower chest-drop/3 band resolves a smooth blend toward 10 mm outer clearance
on the actual garment, leaving approximately 3 mm underside allowance. Normals
come from the same tapered field, including its slope. Whole rows are replaced
transactionally; unsupported rows retain their prior fit. No new subdivision,
dependency or per-frame solver is introduced, and harness recipes do not opt in.

Sixteen shoulder tests pass, including lower attachment checks across 18 body
fixtures, the unchanged complete-shell 2–25 mm clearance gate, exact unchanged
upper rows and bridge, unchanged topology and empty-support handling. Twenty-one
garment tests, strict Clippy, native build, formatting and architecture pass.
The lower-end check is a sampled field-distance witness, not a universal physical
gap or continuous-animation proof. Native review and construction timings are
recorded separately before final acceptance.


Independent native review accepts all 18 matched pairs (36 images), male/female
Hobgoblins in three builds at rest-front, rest-side and one walking-side sample.
The improvement is modest: tabs sit closer, not completely flush. No new crease,
hole, pinch or upper-shoulder change is visible. The existing female underarm
gray oval is unchanged and remains separate follow-up work.

Eight alternating construction runs (one warmup pair, three measured pairs)
give total medians 480.635 to 462.112 ms and torso-recipe medians 203.711 to
193.181 ms. Ranges overlap: totals 472.971–561.751 versus 456.813–492.211 ms;
torso recipes 199.923–283.876 versus 192.695–214.507 ms. This small noisy sample
does not establish a speedup, but shows no material sampled regression.
Both retain 178708 generated triangles, 99306 unique vertices and 25 meshes.
There is no new per-frame operation or FPS claim. All checks are local, not CI.


### Low-shell armhole ownership candidate (2026-11-12)

Native female Hobgoblin side views exposed gray torso foundation through the
leather vest. The side cut, not a separate accessory, removed the outer shell:
analytic witnesses at y=0.362 protruded 15.02, 9.66 and 1.06 mm past its constant
lateral plane in small/default/broad builds, while the same uncut leather
enclosed them by 9.79, 8.88 and 8.05 mm. Both surfaces share the torso joint.
The first-red lower-rib coverage regression also found insufficient clearance
on a male fixture; this is not a female-only special case.

The candidate applies sleeveless side clips only when resolved shell top is
above the existing ARMPIT landmark. A low vest or underbust shell already ends
below the armhole and needs no full-height shoulder cut. This removes two
irrelevant operations rather than adding a new fitter, duplicate support mesh
or collision solver. High shells retain their exact existing opening path.
Hobgoblin/Orc and low Tavern/female Witch shells are affected; high Human/Scout
controls and high male Witch must retain their prior geometry.

Five foundation tests, 16 shoulder tests, 21 garment tests and strict Clippy pass.
The new contracts cover lower native rib coverage and side-cut threshold/low-shell
equivalence. Native control renders and construction measurements remain required
before acceptance. No universal motion or inter-layer clearance claim is made.


Independent full-resolution review accepts all 36 matched pairs (72 images):
six Hobgoblin sex/build fixtures, two Orc fixtures, female Tavern/Witch and
high Human/Scout controls, each at rest-front, rest-side and one walking-side
sample. The female Hobgoblin oval and corresponding Orc side/rear interruptions
are removed without a new sampled arm-contact artifact. Low corsets also regain
a rounder lower-side/hem/belt silhouette; this is broader than armpit pixels but
visually favorable. Tavern/Witch retain their underbust and sleeve boundaries.
All six high-control images are unchanged. This does not qualify continuous gait
or all joint collisions. Existing unrelated elbow, cuff and waist-strap issues
remain separate.

Eight alternating construction runs (one warmup pair, three measured pairs)
give total medians 485.604 to 495.295 ms and torso-recipe medians 206.427 to
218.282 ms. No speedup is claimed. The sample is noisy: total ranges are
471.899–771.499 versus 485.830–637.352 ms, torso ranges 201.455–469.090 versus
203.665–226.652 ms. The restored side surface increases generated triangles
178708 to 180372 (+1664, about 0.93%) and unique vertices 99306 to 100122 (+816);
25 meshes remain unchanged. Accepted for the visible coverage improvement with
these costs disclosed. No per-frame solver was added; no FPS or hosted-CI claim.

### Low-shoe straps use actual calf support (2026-11-13)

Ankle and gladiator bands now reuse the rounded-band component already used by
bracelets rather than independent nominal circular girths. Each band resolves
against the actual calf and retained leg-armor mesh; all bands are resolved before
any emission to prevent self-support. The lowest band now clears the actual emitted shoe upper in the ankle frame;
ankle position alone was insufficient for the taller heeled upper. Band count,
original top extent, material and 6 mm tube half-width/height remain, while
spacing adapts to the available calf span. The 16-by-4 closed rings use more
triangles than the previous 12-by-3 torus to keep facets clear of the calf. This is
construction-time fitting, not a per-frame solver or a new anatomy abstraction.
The initial full-mesh query approach was rejected for its roughly 0.6-second
extra cold-shin construction cost. The shared fitter now conservatively retains
only triangles whose height bounds overlap the exact query window; no support
surface is approximated. A transformed two-layer witness compares every query
with the unfiltered mesh. Final visual review and cost evidence are recorded
separately before acceptance.

Final local qualification: 23 focused methods pass (the actual shoe/calf matrix,
low-shoe and boot-removal controls, rounded-band window equivalence and existing
bracelet/forearm-metal consumers). Strict Clippy, native build, formatting,
architecture and 20 documentation checks pass. Independent review accepts all
32 paired views across six female and two male Human/Orc cases, including bare
calves, armor, closed Witch shoes and an unchanged native boot control. The
shoe-side colored islands are gone and all seven bands remain distinct. All 32
sampled joint/composition states match; this is not a full-cycle motion proof.

The conservative support window reduces the initial rejected fitting cost
substantially. A cheaper 16-by-4 ring was then selected over the visually qualified
24-by-6 trial: the sharper bevels suit the low-poly style across all 32 reviewed
views. Three alternating measured local construction pairs after warmup give
both-shin medians 21.702 ms baseline and 46.144 ms fitted (ranges 20.368–24.412
and 45.739–47.712 ms). The rejected unfiltered approach took 559.860–668.349 ms;
the preserved 24-by-6 trial median was 48.907 ms in its separate comparison.
Whole-model timings are noisy and do not establish a speedup. This is a bounded
quality tradeoff: about 24 ms extra cold construction for correctly seated bands,
not per-frame cost. Seven straps on both legs add 784 generated triangles
(128 versus 72 per band), rather than the 3024 of the denser trial; a single pair
of ankle bands adds 112. No FPS, hosted CI, rebase or merge claim is made.

### Shared band fitting: per-height query windows (2026-11-13)

The rounded-band fitter keeps its 96 angles and nine axial samples, but now
conservatively selects triangles at each sampled height before issuing that
row's horizontal rays. It reuses the existing window predicate, retains any
triangle crossing the slice even if its vertices are outside, and rejects the
entire band if any sample has no support. Full-band support still validates
arbitrary facet heights for relief limits. No new solver, anatomy proxy, reduced
sampling or per-frame work is introduced.

The transformed layered support witness compares full, band-wide and per-height
radii; another witness checks crossing triangles, boundary slack, absent ray
hits and incomplete axial support. Five component methods, the actual calf/armor
matrix, nine bracelet methods and seven forearm-metal methods pass. Prior two
low-shoe controls remain valid for unchanged shoe/stack code. Strict all-target
Clippy and native build pass. Native visual qualification is retained in the
checkpoint acceptance record.

In three alternating isolated measured pairs after warmup, cold two-shin medians
fall from 47.259 to 34.500 ms (ranges 46.638–50.130 and 34.303–36.650 ms), about
27 percent in this fixture. Mesh resolution and triangle cost are unchanged.
This reduces but does not erase the extra cost of correct band fitting; no FPS
or general whole-model speedup is claimed. Earlier qualified compact fitting and
unqualified optimization WIP checkpoints remain preserved. No hosted CI claim.
