# Step-by-step calibration

Calibrate only after the camera, lens, resolution, field of view and ball mounting are fixed. The legacy values and coordinate conventions follow FicTrac; the graphical frontend calls the same shared operations as command-line agents.

## One task at a time

The calibration inspector separates **1 Ball → 2 Exclusions → 3 Lab**. Exclusions are optional. Only the selected step's marks are shown and editable, so checking the rim does not accidentally change a lab corner. Choose **Place new points** for a fresh outline/polygon/square, or **Edit saved marks** to move existing points.

During calibration a cursor-following **4× view magnifier** enlarges the current preview four times, accounting for crop and display scale, and shows a crosshair at the precise source coordinate. It is an aid for positioning marks, not an image rescale applied to the tracker. The full-source/cropped-ball toolbar control lets you recover context without changing calibration. Use **Fit / apply**, **Clear draft**, **Next step** and **Done / resume** to complete or revise one step. The **Cursor magnifier** toggle controls the zoom aid. Hover controls for instructions and shortcuts.

![Actual native calibration inspector with guided Ball step and cursor magnifier](images/calibration.png)

The screenshot uses the attributed sample video; its marks belong to that source.

## 1. Source and projection

Select the actual native resolution and frame rate, and verify the delivered dimensions. Set `vfov` to the vertical field of view in degrees. Select `fisheye` only for the supported equidistant lens model; ordinary rectilinear lenses use the pinhole model. A guessed field of view changes the angular geometry and can bias motion even when the rim appears reasonable.

## 2. Ball rim

Pause on a sharp frame. Select **Ball rim** and click at least three well-spaced points on the visible ball silhouette. The active tool adds only rim points. Fit the rim to update `roi_circ`, `roi_c` and `roi_r` together. The fit is in camera ray space, rather than an arbitrary screen-space circle.

Right-click near a point to remove that point; use the point-movement tool to correct saved geometry. Avoid points on the animal, ball support or a shadow mistaken for the silhouette. Inspect the full outline after fitting, including portions between the clicked points.

## 3. Exclusions

Select **Exclude**, place a polygon around an obstruction and finish it with Enter. Add further regions where needed. Right-click deletes the nearest relevant point. Excluded source pixels do not contribute to the numerical fit. The tool changes only its selected geometry; it does not add rim or lab-square points incidentally.

Mask the animal and stationary hardware while preserving enough patterned sphere for a distinctive fit. An overly large mask can make the optimizer underconstrained. Exclusions are a static setup aid; they do not reconstruct a temporary hand occlusion.

## 4. Lab square and forward direction

Use a square physically aligned with the chosen animal plane: **XY**, **YZ** or **XZ**. Enter the four corners in the shown order **top-left → top-right → bottom-right → bottom-left** for the selected reference view. The exact plane-specific 3D correspondence is listed in [agent calibration](agent-control.md). All four corners stay visible for inspection. Replacing a corner keeps its original slot; swapping corner order changes the correspondence.

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

The same resolution operations are available headlessly:

```json
{"id":2,"command":"calibrate","session":1,"args":{"kind":"resolution_reference"}}
{"id":3,"command":"calibrate","session":1,"args":{"kind":"resolution_apply","width":1280,"height":960}}
```

Reference capture can optionally specify width/height; otherwise it uses the current source. Discover the precise current schema with `hejjetrak-cli schema` before constructing requests; the [agent chapter](agent-control.md) documents the operations and transactional failure behavior. Source coordinates are independent of preview zoom and window size.
