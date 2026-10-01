// A drum-fed gun on Frog's .45 rounds.
datablock ItemData(ThompsonItem : standinSidearmItem)
{
   uiName = "Stand-in Drum Gun";
   image = ThompsonImage;
   TT_ammoType = "45Caliber";
   TT_maxAmmo = 25;
};

datablock ShapeBaseImageData(ThompsonImage : standinSidearmImage)
{
   item = ThompsonItem;
};
