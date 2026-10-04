Own-player Pose and VehiclePose.motion carry the authoritative cumulative PassageFrame; own Pose pairs a mounted vehicle ID/frame with the rider body frame at the same tick.
Prediction and camera reconciliation use actual accepted trips, including carried riders and target changes, instead of inferring fate from positions or current portal links. Alpha clients must use this protocol together.
