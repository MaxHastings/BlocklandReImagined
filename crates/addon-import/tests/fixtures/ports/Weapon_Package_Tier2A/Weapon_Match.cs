// The easter egg, run only with the hidden setting.
datablock ItemData(MatchPistolItem : standinSidearmItem)
{
   uiName = "Stand-in Match";
   image = MatchPistolImage;
   TT_ammoType = "880";
   TT_maxAmmo = 3;
};

datablock ShapeBaseImageData(MatchPistolImage : standinSidearmImage)
{
   item = MatchPistolItem;
   TT_raycastDirectDamage = 18;
   TT_raycastDirectDamageType = $DamageType::StandinMatch;
};

function MatchPistolImage::onFire(%this,%obj,%slot)
{
   Parent::onFire(%this,%obj,%slot);
   %this.TT_decrementAmmo(%obj);
}

function MatchPistolImage::TT_isRaycastCritical(%this, %obj, %slot, %col, %pos, %normal, %hit)
{
	return TT_isRaycastHeadshot(%this, %obj, %slot, %col, %pos, %normal, %hit);
}
