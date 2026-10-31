Rule music notices carry a finite, bounded per-track volume alongside track,
looping and fade. The package API defaults omitted volume to 1.0; receivers
multiply it with the sound profile and local Music/master controls. Alpha wire
change: MusicPlay adds volume.
