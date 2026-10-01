// The launcher: whoever holds it walks slowly.
datablock ItemData(PayloadLauncherItem : standinSidearmItem)
{
   uiName = "Stand-in Launcher";
   image = PayloadLauncherImage;
   TT_ammoType = "Payload";
   TT_maxAmmo = 1;
};

datablock ShapeBaseImageData(PayloadLauncherImage : standinSidearmImage)
{
   item = PayloadLauncherItem;
   TT_raycastEnabled = false;
};

function PayloadLauncherImage::onMount(%this,%obj,%slot)
{
   if($Pref::Server::FW::PayloadSlow)
      %obj.pushDatablock(SlowedArmor.getID());
   Parent::onMount(%this,%obj,%slot);
}

function PayloadLauncherImage::onUnMount(%this,%obj,%slot)
{
   %obj.popDatablock(SlowedArmor.getID());
   Parent::onUnMount(%this,%obj,%slot);
}
