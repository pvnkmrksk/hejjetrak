# Step-by-step calibration

Fix the camera, lens, resolution and ball mounting first. Calibration then follows **Ball → Exclusions → Lab**, with one layer active at a time. The image banner explains the current task and the marks still needed. The app and CLI use the same Rust fitting operations.

The workflow below is included in v0.1.4 and newer. Earlier packages use the previous review/fit controls.

## Begin or redo calibration

Choose **Calibrate again…** from the preview context menu or Settings. The app pauses and immediately opens a clear ball-outline draft. There is no extra Fit action or saved-point editing step.

- Left-click to place a mark. **Right-click a mark to remove it**, then click its replacement.
- **Start this layer over** clears the active draft so you can redraw it.
- **Continue** accepts a valid draft as one undoable edit and opens the next layer. **Finish & resume** completes the Lab step. Enter also continues when the image has focus.
- **Keep saved & continue** or **Keep saved & finish** skips the current layer, discards its draft and preserves the saved values.
- **Cancel** or Escape discards the current draft and returns to the running or paused state from before calibration. Previously accepted layers remain in memory and can be undone.

Incomplete or invalid drafts cannot replace a saved calibration. Saving to disk is explicit. A tiny preview expands to a useful calibration canvas, then returns to its previous size. This changes only the window size.

## 1. Ball outline

Set `vfov` to the vertical field of view of the **whole delivered image**, in degrees. It is not the ball diameter or the lens’s diagonal FOV. The [hardware guide](hardware.md#measure-field-of-view-before-mounting) explains a bench measurement. Select the supported equidistant `fisheye` model only for a matching lens; ordinary rectilinear lenses use the pinhole model.

Click **at least three well-separated points on the ball silhouette**. The green fitted outline appears automatically after the third valid point and updates with every further mark. Rim points are never joined by straight segments. Two points cannot define a unique circle; place the third away from their line. Add points around the visible rim rather than clustering them together.

Check the outline between the marks. Avoid the animal, supports and shadows. If a mark is wrong, right-click it and place it again. Choose **Continue** when the outline follows the ball. This commits `roi_circ`, `roi_c` and `roi_r` together, using the selected camera projection and FOV.

The **Precision magnifier** gives a 240-pixel view at 4× the current preview scale, with the fitted outline, numbered marks and source-coordinate crosshair. It appears in a mouse-transparent native window where space permits, or as an inset away from the cursor. Full-image/cropped view changes retain the source aspect ratio and numerical coordinates.

![Native live rim fit after three marks](images/calibration.png)

## 2. Exclusions

The next step opens an empty exclusion draft. Click around the animal, tether or stationary hardware. Straight edges are appropriate here: exclusions are polygons. A closed preview appears after the third point. Right-click a wrong point to remove it.

Choose **Add this region** to accept one polygon and draw another. **Continue** accepts the current valid region and opens the Lab step. In Settings, **Use no exclusions** explicitly clears the masks. Use **Keep saved & continue** to retain the existing masks instead of redrawing them.

Preserve enough patterned sphere to identify its orientation. An overly large mask leaves too little information for tracking. Exclusions describe fixed obstructions; they do not recover motion during a temporary occlusion.

## 3. Lab axes

Choose the plane of a square physically aligned with the animal: **XY**, **YZ** or **XZ**. Place its four corners in the displayed order **top-left → top-right → bottom-right → bottom-left** for the selected reference view. The [agent reference](agent-control.md) gives the exact plane-specific 3D correspondence.

The fitted lab axes appear automatically after the fourth corner. Right-click a misplaced corner and click its replacement; it retains its original slot. Inspect the axis directions, then choose **Finish & resume** to update `c2a_r`/`c2a_t`. **Keep saved & finish** preserves the existing lab setup.

![FicTrac camera, animal and world coordinate frames](images/upstream/coordinate-frames.png)

*Original FicTrac illustration by R. J. D. Moore and contributors, CC BY-NC-SA 3.0. [Coordinate guide](upstream-coordinates.md) · [Source and unchanged-image checksums](https://github.com/pvnkmrksk/hejjetrak/blob/main/docs/images/upstream/README.md).*

Before collecting data, rotate the ball in known forward, sideways and turning directions and confirm the result. A smooth-looking path alone does not establish the axis orientation.

## Review or redraw one layer

**Diagnostics** shows the complete input image with the saved ball outline, exclusion masks, square and axes. Use **Redraw ball**, **Redraw exclusions** or **Redraw lab** to replace just that layer. **Full configuration…** opens the complete config text in the inspector.

For occasional point adjustments, Settings also offers **Review saved layer** and **Edit saved marks**. This secondary editor supports dragging and right-click deletion. If a file contains only fitted angular geometry or rotation, there are no historical points to edit; use Redraw instead. In the saved Lab editor, **F** tries the reflected-seed alternate pose. A distinct valid branch is not always available; failure preserves the prior configuration.

Check the thresholded ROI and map for useful contrast, then save the config for that physical rig. Saved `roi_c`/`roi_r` and `c2a_r` take precedence over old click fields. Editing only old points as raw text does not implicitly refit the saved geometry: use the corresponding calibration tool.

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

Reference capture can optionally specify width/height; otherwise it uses the current source. Discover the precise current schema with `hejjetrac-cli schema` before constructing requests; the [agent chapter](agent-control.md) documents the operations and transactional failure behavior. Source coordinates are independent of preview zoom and window size.
