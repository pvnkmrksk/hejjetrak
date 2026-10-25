# Step-by-step calibration

Calibrate only after the camera, lens, resolution, field of view and ball mounting are fixed. The legacy values and coordinate conventions follow FicTrac; the graphical frontend calls the same shared operations as command-line agents.

## One task at a time

Choose **Calibrate again…** to review the saved setup in order: **Ball →
Exclusions → Lab**. Each step starts read-only with a summary of its existing
marks. Only its geometry is highlighted. You can keep a working calibration
without drawing over it or rewriting its fields.

- **Keep & continue / Keep & finish** preserves the step exactly and advances.
- **Edit saved marks** enables dragging/right-click deletion for this step. When
  only fitted angular geometry/rotation was saved and no clicked points exist,
  use Keep or Redraw; there are no historical click points to edit.
- **Redraw this step** starts an isolated replacement draft and hides only this
  step's old geometry. A valid applied replacement is one undoable edit; other
  steps remain literally unchanged. Cancel returns to saved review. Applied
  edits can be reversed with Undo.

A tiny preview temporarily expands to a 640×480 calibration canvas; finishing
returns to its previous preview size. This changes only the window, not source
resolution, projection or calibration coordinates.

Exclusions also offer **Add another region**, which appends instead of replacing
the saved masks, and **Use no exclusions**, an explicit clear operation. A failed
rim, polygon or square fit preserves the saved configuration.

The **Precision magnifier** provides a 240-pixel nearest-pixel view at 4× the
current preview scale. It shows the fitted outline/active polygon, numbered
marks or TL/TR/BR/BL corner labels and the source-coordinate crosshair. When
screen space permits it uses a mouse-transparent native window outside the
preview; otherwise it appears as an inset away from the cursor. This is a
placement aid, not a source-image resize applied to tracking. Full-image/cropped
view switches retain the source aspect ratio.

![Actual native saved-calibration preview](images/calibration.png)

This v0.1.3 native capture shows the full source and saved ball outline in read-only review. The separate inspector was visually checked and its Keep & continue button exercised. Automated source-coordinate tests cover point placement, deletion and magnifier layout; the native hover/loupe pointer check remains incomplete because desktop automation lost its window binding.

## 1. Source and projection

Select the actual native resolution and frame rate, and verify the delivered dimensions. Set `vfov` to the vertical field of view in degrees. Select `fisheye` only for the supported equidistant lens model; ordinary rectilinear lenses use the pinhole model. A guessed field of view changes the angular geometry and can bias motion even when the rim appears reasonable.

## 2. Ball rim

Pause on a sharp frame. In Ball review, choose **Redraw this step**, then click at least three well-spaced points on the visible ball silhouette. The active tool adds only rim points. Fit the rim to update `roi_circ`, `roi_c` and `roi_r` together. The fit is in camera ray space, rather than an arbitrary screen-space circle.

In Edit saved marks, drag a point to correct it or right-click nearby to remove it. Review mode itself does not change marks. Avoid points on the animal, ball support or a shadow mistaken for the silhouette. Inspect the full outline after fitting, including portions between the clicked points.

## 3. Exclusions

In Exclusions review, choose **Add another region** to retain existing masks, or **Redraw this step** for a replacement. Place a polygon around an obstruction and finish/apply it. Use **Use no exclusions** when no masks are needed. Right-click deletes the nearest relevant point. Excluded source pixels do not contribute to the numerical fit. The tool changes only its selected geometry; it does not add rim or lab-square points incidentally.

Mask the animal and stationary hardware while preserving enough patterned sphere for a distinctive fit. An overly large mask can make the optimizer underconstrained. Exclusions are a static setup aid; they do not reconstruct a temporary hand occlusion.

## 4. Lab square and forward direction

In Lab review, Keep the existing setup, Edit its saved corners, or Redraw a square physically aligned with the chosen animal plane: **XY**, **YZ** or **XZ**. Enter the four corners in the shown order **top-left → top-right → bottom-right → bottom-left** for the selected reference view. The exact plane-specific 3D correspondence is listed in [agent calibration](agent-control.md). All four corners stay visible for inspection. Replacing a corner keeps its original slot; swapping corner order changes the correspondence.

The square fit computes `c2a_r`/`c2a_t`. Press **F** for the reflected-seed alternate-pose refit when the current planar branch is inappropriate. A valid distinct branch is not always available; a failed alternate fit leaves the prior configuration intact. The implementation uses a bounded deterministic Rust fit and is not identical to upstream NLopt.

Verify the forward/side axes with known physical ball motion. Do not infer correctness from the sign of an arbitrary walking trajectory.

## 5. Review, save and resume

Check the complete source view with guides enabled. Review the thresholded ROI and map for useful contrast. Save explicitly to the rig's own config, then resume. Preview crop/full-view switches affect only display; numerical calibration remains in source geometry.

Saved `roi_c`/`roi_r` take precedence over historical rim-click coordinates. Saved `c2a_r` similarly takes precedence over square clicks. Editing only the raw old points does not implicitly refit those saved values: use the corresponding calibration operation.

## Resolution and framing changes

Legacy `roi_circ`, `roi_ignr` and lab-square corner fields are source **pixel** coordinates. The fitted sphere centre and radius are angular quantities. Old FicTrac files do not encode a universal normalized-image coordinate system. Changing resolution must therefore not silently reinterpret every old number as a fraction.

With an unchanged lens and field of view, angular centre/radius can remain meaningful after a proportional resize, while pixel masks/corners do not. A crop, aspect-ratio change, sensor mode change, digital stabilization or zoom can alter the projection itself. Inspect/refit after those changes.

### Opt in to a resolution reference

1. At the source resolution where the calibration is correct, expand the resolution reference section and choose **Use W × H as calibration reference**. This saves the reference width/height, projection and field of view, plus normalized copies of rim, exclusion and lab-square pixel marks. Original legacy pixel fields remain present and keep their ordinary meaning.
2. For a proportional resolution change with the **same aspect ratio, configured field of view and lens model**, choose **Adapt saved marks to W × H**. The shared operation regenerates the legacy pixel marks from that explicit reference. It leaves angular `roi_c`/`roi_r` and lab `c2a_r`/`c2a_t` unchanged.
3. Review the displayed result, then save explicitly. The camera-format UI can apply the same scale operation during a format change only when opted-in metadata exists.

Legacy imports are never silently normalized. Stale reference marks, changed aspect ratio, field of view or projection reject atomically rather than inventing a new calibration. The helper handles proportional source dimensions; it cannot detect a camera mode's undocumented sensor crop, digital zoom or changed optics. Those need a new physical check/refit.

Metadata uses `rust_calibration_size`, `rust_calibration_vfov`, `rust_calibration_fisheye`, and `rust_normalized_roi_circ`, `rust_normalized_roi_ignr`, `rust_normalized_c2a_cnrs_xy`, `rust_normalized_c2a_cnrs_yz`, `rust_normalized_c2a_cnrs_xz`. These are extensions; an older FicTrac continues to read the ordinary regenerated pixel fields.

## Headless equivalent

```json
{"id":1,"command":"calibrate","session":1,"args":{"kind":"rim","points":[[100,80],[160,55],[220,80],[240,140]]}}
```

To replace the whole exclusion set atomically (an empty array clears it):

```json
{"id":2,"command":"calibrate","session":1,"args":{"kind":"replace_exclusions","polygons":[[[80,90],[100,90],[100,110],[80,110]]]}}
```

Native displayed sessions additionally expose the review state through `view`:

```json
{"id":3,"command":"view","session":1,"args":{"calibration_step":"ball","calibration_action":"inspect"}}
{"id":4,"command":"view","session":1,"args":{"calibration_action":"keep"}}
```

The same resolution operations are available headlessly:

```json
{"id":5,"command":"calibrate","session":1,"args":{"kind":"resolution_reference"}}
{"id":6,"command":"calibrate","session":1,"args":{"kind":"resolution_apply","width":1280,"height":960}}
```

Reference capture can optionally specify width/height; otherwise it uses the current source. Discover the precise current schema with `hejjetrak-cli schema` before constructing requests; the [agent chapter](agent-control.md) documents the operations and transactional failure behavior. Source coordinates are independent of preview zoom and window size.
