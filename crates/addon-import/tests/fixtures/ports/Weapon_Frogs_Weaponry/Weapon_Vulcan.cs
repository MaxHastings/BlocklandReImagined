// The spinner: it slows its gunner as it spins up, and jet roots it.
datablock ItemData(VulcanItem : standinSidearmItem)
{
   uiName = "Stand-in Spinner";
   image = VulcanImage;
   TT_ammoType = "50Cal";
   TT_maxAmmo = 40;
};

datablock ShapeBaseImageData(VulcanImage : standinSidearmImage)
{
   item = VulcanItem;
   TT_raycastEnabled = false;

   stateTransitionOnTriggerDown[1]  = "SpinUp";
   stateTransitionOnTriggerUp[5]    = "Restore";

   stateName[14]                    = "SpinUp";
   stateScript[14]                  = "onSpinUp";
   stateTimeoutValue[14]            = 0.25;
   stateTransitionOnTimeout[14]     = "FireCheckA";

   stateName[15]                    = "Restore";
   stateScript[15]                  = "onRestore";
   stateTimeoutValue[15]            = 0.05;
   stateTransitionOnTimeout[15]     = "LoadCheckA";
};

function VulcanImage::onSpinUp(%this,%obj,%slot)
{
   if($Pref::Server::FW::VulcanSlow)
      %obj.pushDatablock(SlowedArmor.getID());
}

function VulcanImage::onRestore(%this,%obj,%slot)
{
   %obj.popDatablock(SlowedArmor.getID());
}

function VulcanImage::onUnMount(%this,%obj,%slot)
{
   %obj.popDatablock(SlowedArmor.getID());
   Parent::onUnMount(%this,%obj,%slot);
}

function VulcanImage::onAltTrigger(%this,%obj,%objDB,%triggerSlot,%val)
{
   if(%val && %triggerSlot == 4 && $Pref::Server::FW::VulcanDeploy)
   {
      %obj.mountImage(VulcanDeployedImage, 0);
      %obj.pushDatablock(DeployedArmor.getID());
   }
}

datablock ShapeBaseImageData(VulcanDeployedImage : VulcanImage)
{
   item = VulcanItem;
};

function VulcanDeployedImage::onMount(%this,%obj,%slot)
{
   Parent::onMount(%this,%obj,%slot);
   %obj.pushDatablock(DeployedArmor.getID());
}

function VulcanDeployedImage::onUnMount(%this,%obj,%slot)
{
   Parent::onUnMount(%this,%obj,%slot);
   %obj.popDatablock(DeployedArmor.getID());
}

function VulcanDeployedImage::onAltTrigger(%this,%obj,%objDB,%triggerSlot,%val)
{
   if(%val && %triggerSlot == 4)
   {
      %obj.mountImage(VulcanImage, 0);
      %obj.popDatablock(DeployedArmor.getID());
   }
}
