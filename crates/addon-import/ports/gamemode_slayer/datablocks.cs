// Slayer makes its countdown voices at run time in Sounds.cs, one
// AudioProfile per second it counts (a loop that renames `slayerSound`).
// Here they are declared as the importer reads them, so the rules can play
// each by id. The count, ten, is pinned against this copy's Sounds.cs.

datablock AudioProfile(Slayer_1_Seconds_Sound) { filename = "./server/core/resources/sounds/1.wav"; description = AudioClosest3d; preload = false; };
datablock AudioProfile(Slayer_2_Seconds_Sound) { filename = "./server/core/resources/sounds/2.wav"; description = AudioClosest3d; preload = false; };
datablock AudioProfile(Slayer_3_Seconds_Sound) { filename = "./server/core/resources/sounds/3.wav"; description = AudioClosest3d; preload = false; };
datablock AudioProfile(Slayer_4_Seconds_Sound) { filename = "./server/core/resources/sounds/4.wav"; description = AudioClosest3d; preload = false; };
datablock AudioProfile(Slayer_5_Seconds_Sound) { filename = "./server/core/resources/sounds/5.wav"; description = AudioClosest3d; preload = false; };
datablock AudioProfile(Slayer_6_Seconds_Sound) { filename = "./server/core/resources/sounds/6.wav"; description = AudioClosest3d; preload = false; };
datablock AudioProfile(Slayer_7_Seconds_Sound) { filename = "./server/core/resources/sounds/7.wav"; description = AudioClosest3d; preload = false; };
datablock AudioProfile(Slayer_8_Seconds_Sound) { filename = "./server/core/resources/sounds/8.wav"; description = AudioClosest3d; preload = false; };
datablock AudioProfile(Slayer_9_Seconds_Sound) { filename = "./server/core/resources/sounds/9.wav"; description = AudioClosest3d; preload = false; };
datablock AudioProfile(Slayer_10_Seconds_Sound) { filename = "./server/core/resources/sounds/10.wav"; description = AudioClosest3d; preload = false; };
