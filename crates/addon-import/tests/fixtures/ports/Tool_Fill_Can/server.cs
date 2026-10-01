// Stand-in for the Tool_Fill_Can port tests (CC0): a self-contained paint
// tool whose functions have the shapes the Fill Can port reads.
datablock ProjectileData(fillcanProjectile)
{
   muzzleVelocity      = 20;
   velInheritFactor    = 0;
   lifetime            = 500;
   isBallistic         = false;
   gravityMod          = 0.0;
};

datablock ItemData(fillcanItem)
{
   category = "Weapon";
   className = "Weapon";
   shapeFile = "./can.dts";
   uiName = "Stand-in Fill Can";
   image = fillcanImage;
   canDrop = true;
};

datablock ShapeBaseImageData(fillcanImage)
{
   shapeFile = "./can.dts";
   mountPoint = 0;
   className = "WeaponImage";
   item = fillcanItem;
   projectile = fillcanProjectile;
   projectileType = Projectile;
   minShotTime = 100;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.1;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Fire";
   stateAllowImageChange[1]        = true;

   stateName[2]                    = "Fire";
   stateTransitionOnTimeout[2]     = "Ready";
   stateTimeoutValue[2]            = 0.1;
   stateFire[2]                    = true;
   stateScript[2]                  = "onFire";
   stateWaitForTimeout[2]          = true;
};

function paintfill(%client, %obj, %newcolor)
{
   %oldcolor = %obj.getColorID();
   if(%client.isAdmin)
      %limit = 3;
   else
      %limit = $Pref::Server::FillCan_MaxBricks;
   %list = new ScriptObject() { count = 0; };
   %list.obj[-1 + %list.count++] = %obj;
   for(%x = 0; %x < %list.count; %x++)
   {
      %obj = %list.obj[%x];
      %obj.setColor(%newcolor);
      %fillcount++;
      if(%fillcount >= %limit) { messageClient(%client, 'MsgPlantError_Limit'); centerPrint(%client, "Reached Fill Can Brick Limit (" @ $Pref::Server::FillCan_MaxBricks @ ")", 4, 1); break; }
      %data = %obj.getDataBlock();
      %box = (%sizex * 0.5 + 0.6) SPC (%sizey * 0.5 + 0.6) SPC (%sizez * 0.2 + 0.3);
      InitContainerBoxSearch(%obj.getPosition(), %box, $TypeMasks::FxBrickObjectType);
      while(%target = containerSearchNext())
         if(%target.getColorID() == %oldcolor && getTrustLevel(%client, %target) >= 2)
            %list.obj[-1 + %list.count++] = %target;
   }
   %client.undoStack.push(%list TAB "FILLPAINT");
}

function paintFXfill(%client, %obj, %newfx)
{
   %oldcolor = %obj.getColorID();
   for(%x = 0; %x < %list.count; %x++)
   {
      %obj.setColorFX(%newfx);
      if(%fillcount >= %limit) { break; }
      InitContainerBoxSearch(%obj.getPosition(), %box, $TypeMasks::FxBrickObjectType);
      while(%target = containerSearchNext())
         if(%target.getColorID() == %oldcolor && getTrustLevel(%client, %target) >= 2)
            %list.obj[-1 + %list.count++] = %target;
   }
}

function shapeFXfill(%client, %obj, %newfx)
{
   %oldcolor = %obj.getColorID();
   for(%x = 0; %x < %list.count; %x++)
   {
      %obj.setShapeFX(%newfx);
      if(%fillcount >= %limit) { break; }
      InitContainerBoxSearch(%obj.getPosition(), %box, $TypeMasks::FxBrickObjectType);
      while(%target = containerSearchNext())
         if(%target.getColorID() == %oldcolor && getTrustLevel(%client, %target) >= 2)
            %list.obj[-1 + %list.count++] = %target;
   }
}

function fillcanProjectile::onCollision(%this, %obj, %col, %fade, %pos, %normal)
{
   %client = %obj.client;
   if(%col.getClassName() $= "fxDTSBrick")
   {
      paintfill(%client, %col, %client.currentColor);
   }
   else if(%col.getClassName() $= "Player")
   {
      if(%client.currentFXcan)
      {
         %col.faceResetSched = schedule(1400, 0, resetface, %col);
         %col.setNodeColor("visor", getWords(%col.client.accentColor, 0, 2) SPC "0.6");
      }
      else
      {
         %color = getColorIDTable(%client.currentColor);
         %col.setTempColor(%color, 1200);
      }
   }
   else if(getTrustLevel(%client, %col.spawnBrick) >= 2)
   {
      if(%client.currentFXcan)
         %col.color = (getRandom(0, 100) / 100) SPC (getRandom(0, 100) / 100) SPC (getRandom(0, 100) / 100) SPC "1";
      else
      {
         %col.spawnBrick.setColor(%client.currentColor);
         for(%x = 0; %x < %col.getMountedObjectCount(); %x++)
            %col.getMountedObject(%x).setTempColor(%col.color, 1300);
      }
   }
   else
   {
      %name = %col.spawnBrick.getGroup().name;
      centerPrint(%client, %name @ " does not trust you enough to do that.", 2, 1);
   }
}

function resetface(%obj)
{
   %obj.setFaceName(%obj.client.faceName);
}

function serverCmdFillCan(%client)
{
   if(isObject(%client.minigame) && !%client.minigame.enablePainting)
   {
      centerPrint(%client, "\c5Painting is currently disabled.", 5, 1);
      return;
   }
   %client.player.mountImage(fillcanImage, 0);
}

package FillCanStandIn
{
   function serverCmdUndoBrick(%client)
   {
      %str = %client.undoStack.val[%client.undoStack.head - 1];
      if(getField(%str, 1) $= "FILLPAINT")
      {
         %b = %list.obj[%x];
         if(%b.getColorID() == %list.newcolor)
            %b.setColor(%list.oldcolor);
      }
      else
         Parent::serverCmdUndoBrick(%client);
   }
   function serverCmdUseFXCan(%client, %can)
   {
      Parent::serverCmdUseFXCan(%client, %can);
      %client.player.mountImage(fillcanImage, 0);
      %client.currentFXcan = %can + 1;
   }
   function serverCmdUseSprayCan(%client, %can)
   {
      Parent::serverCmdUseSprayCan(%client, %can);
      %client.player.mountImage(fillcanImage, 0);
      %client.currentFXcan = 0;
   }
   function GameConnection::onClientLeaveGame(%client)
   {
      if(getWord(%t, 1) $= "FILLPAINT") getWord(%t, 0).delete();
      Parent::onClientLeaveGame(%client);
   }
};
activatePackage(FillCanStandIn);

function round(%a)
{
   %a += 0.5;
   return %a;
}
