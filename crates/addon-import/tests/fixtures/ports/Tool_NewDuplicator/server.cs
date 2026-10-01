// Stand-in for the Tool_NewDuplicator port tests (CC0): a wand in three
// colours with the function shapes the port reads and its own numbers
// (reach 12, 3 bricks for players, an 8-unit box, a 250 ms wait, super
// shifts of 4 studs and 10 plates, names up to 20 letters). The port, not
// this script, is what runs.
datablock ExplosionData(ND_HitExplosion)
{
   lifetimeMS = 100;
};

datablock ProjectileData(ND_HitProjectile)
{
   explosion      = ND_HitExplosion;
   explodeOnDeath = true;
   lifetime       = 0;
   range          = 12;
};

datablock ProjectileData(ND_HitProjectile_Blue : ND_HitProjectile)
{
   range = 12;
};

datablock ItemData(ND_Item)
{
   category  = "Tools";
   className = "Weapon";
   shapeFile = "./wand.dts";
   uiName    = "Stand-in New Duplicator";
   image     = ND_Image;
   canDrop   = true;
};

datablock ShapeBaseImageData(ND_Image)
{
   shapeFile  = "./wand.dts";
   mountPoint = 0;
   item       = ND_Item;
   projectile = ND_HitProjectile;
   armReady   = true;

   stateName[0]                    = "Activate";
   stateTimeoutValue[0]            = 0.1;
   stateTransitionOnTimeout[0]     = "Ready";

   stateName[1]                    = "Ready";
   stateTransitionOnTriggerDown[1] = "Fire";
   stateAllowImageChange[1]        = true;

   stateName[2]                    = "Fire";
   stateTransitionOnTimeout[2]     = "Ready";
   stateTimeoutValue[2]            = 0.2;
   stateFire[2]                    = true;
   stateScript[2]                  = "onFire";
   stateWaitForTimeout[2]          = true;
};

datablock ShapeBaseImageData(ND_Image_Box : ND_Image)
{
   shapeFile = "./box.dts";
};

datablock ShapeBaseImageData(ND_Image_Blue : ND_Image)
{
   projectile = ND_HitProjectile_Blue;
};

function ndApplyDefaultPrefValues()
{
   $Pref::Server::ND::AdminOnly          = false;
   $Pref::Server::ND::SaveAdminOnly      = true;
   $Pref::Server::ND::LoadAdminOnly      = false;
   $Pref::Server::ND::TrustLimit         = 2;
   $Pref::Server::ND::AdminTrustBypass1  = true;
   $Pref::Server::ND::SelectPublicBricks = true;
   $Pref::Server::ND::MaxBricksAdmin     = 2000000;
   $Pref::Server::ND::MaxBricksPlayer    = 3;
   $Pref::Server::ND::MaxBoxSizeAdmin    = 2000;
   $Pref::Server::ND::MaxBoxSizePlayer   = 8;
   $Pref::Server::ND::SelectTimeoutMS    = 250;
   $Pref::Server::ND::PaintAdminOnly      = false;
   $Pref::Server::ND::PaintFxAdminOnly    = false;
   $Pref::Server::ND::WrenchAdminOnly     = false;
   $Pref::Server::ND::FloatAdminOnly      = false;
   $Pref::Server::ND::FillBricksAdminOnly = false;
}

function serverCmdNewDuplicator(%client)
{
   if($Pref::Server::ND::AdminOnly && !%client.isAdmin)
      return messageClient(%client, '', "Admin only.");
   %image = ND_Image;
   %client.player.mountImage(%image, 0);
}

package NewDuplicator_Stand_In
{
   function serverCmdDuplicator(%client) { serverCmdNewDuplicator(%client); }
   function serverCmdD(%client) { serverCmdNewDuplicator(%client); }
   function serverCmdLight(%client) { %client.ndMode.onLight(%client); }
   function serverCmdNextSeat(%client) { %client.ndMode.onNextSeat(%client); }
   function serverCmdPrevSeat(%client) { %client.ndMode.onPrevSeat(%client); }
   function serverCmdShiftBrick(%client, %x, %y, %z) { %client.ndMode.onShiftBrick(%client, %x, %y, %z); }
   function serverCmdSuperShiftBrick(%client, %x, %y, %z) { %client.ndMode.onSuperShiftBrick(%client, %x, %y, %z); }
   function serverCmdRotateBrick(%client, %dir) { %client.ndMode.onRotateBrick(%client, %dir); }
   function serverCmdPlantBrick(%client) { %client.ndMode.onPlantBrick(%client); }
   function serverCmdCancelBrick(%client) { %client.ndMode.onCancelBrick(%client); }
   function ND_Image::onMount(%this, %player, %slot) { %player.ndEquipped(); }
   function ND_Image::onUnMount(%this, %player, %slot) { %player.ndUnEquipped(); }
};
activatePackage(NewDuplicator_Stand_In);

function GameConnection::ndSetImage(%this, %image)
{
   %this.ndIgnoreNextMount = true;
   %this.player.mountImage(%image, 0);
}

function Player::ndEquipped(%this)
{
   if(%this.client.ndIgnoreNextMount)
      return %this.client.ndIgnoreNextMount = false;
   %client = %this.client;
   %client.ndSetMode(%client.ndLastSelectMode);
}

function Player::ndUnEquipped(%this)
{
   %this.client.ndKillMode();
}

function Player::ndFired(%this)
{
   %len[2] = 12;
   %data = ND_HitProjectile;
   %client = %this.client;
   %client.ndMode.onSelectObject(%client, %obj, %pos, %normal);
}

function ndFormatMessage(%title, %l0, %r0)
{
   return "<font:Arial:22>" @ %title @ "\n<font:Verdana:16>" @ %l0 @ "<just:right>" @ %r0;
}

function NDM_StackSelect::onSelectObject(%this, %client, %obj)
{
   if(%client.ndLastSelectTime + $Pref::Server::ND::SelectTimeoutMS / 1000 > $Sim::Time)
      return;
   if(%client.ndMultiSelect)
      %client.ndSelection.startStackSelectionAdditive(%obj, %client.ndDirection, %client.ndLimited);
   else
      %client.ndSelection.startStackSelection(%obj, %client.ndDirection, %client.ndLimited);
}

function NDM_StackSelect::onLight(%this, %client) { %client.ndSetMode(NDM_BoxSelect); }
function NDM_StackSelect::onNextSeat(%this, %client) { %client.ndDirection = !%client.ndDirection; }
function NDM_StackSelect::onPrevSeat(%this, %client) { %client.ndLimited = !%client.ndLimited; }
function NDM_StackSelect::onCancelBrick(%this, %client) { %client.ndSelection.deleteData(); }
function NDM_StackSelect::onCut(%this, %client) { %client.ndSelection.startCutting(); }
function NDM_StackSelect::getBottomPrint(%this, %client)
{
   return ndFormatMessage("Selection Mode", "Direction: Up", "");
}

function NDM_BoxSelect::onSelectObject(%this, %client, %obj, %pos, %normal)
{
   %box = %obj.getWorldBox();
   if(%client.ndMultiSelect)
      %box = ndGetPlateBoxFromRayCast(%pos, %normal);
   %client.ndSelectionBox.setSizeAligned(getWords(%box, 0, 2), getWords(%box, 3, 5), %client.player);
}

function NDM_BoxSelect::onLight(%this, %client) { %client.ndSetMode(NDM_StackSelect); }
function NDM_BoxSelect::onPrevSeat(%this, %client) { %client.ndLimited = !%client.ndLimited; }

function NDM_BoxSelect::onShiftBrick(%this, %client, %x, %y, %z)
{
   %newX = mFloor(%newX) / 2;
   %z = mFloor(%z) / 5;
   %limit = $Pref::Server::ND::MaxBoxSizePlayer;
   %client.ndSelectionBox.shiftCorner(%newX SPC %z, %limit);
}

function NDM_BoxSelect::onSuperShiftBrick(%this, %client, %x, %y, %z)
{
   %this.onShiftBrick(%client, %x * 4, %y * 4, %z * 10);
}

function NDM_BoxSelect::onRotateBrick(%this, %client) { %client.ndSelectionBox.switchCorner(); }
function NDM_BoxSelect::onPlantBrick(%this, %client)
{
   %box = %client.ndSelectionBox.getWorldBox();
   %client.ndSelection.startBoxSelection(%box, %client.ndLimited);
}
function NDM_BoxSelect::onCancelBrick(%this, %client)
{
   commandToClient(%client, 'centerPrint', "Selection canceled!", 5);
}
function NDM_BoxSelect::getBottomPrint(%this, %client)
{
   return ndFormatMessage("Selection Mode", "Size: 1 x 1 x 1 Plates", "");
}

function ND_SelectionBox::setSizeAligned(%this, %point1, %point2, %player)
{
   %angle = getAngleIDFromPlayer(%player);
   %this.point1 = %point1;
   %this.point2 = %point2;
}

function ND_SelectionBox::shiftCorner(%this, %offset, %limit)
{
   if(getWord(%offset, 0) > %limit)
      %limitReached = true;
   return %limitReached;
}

function ND_SelectionBox::switchCorner(%this)
{
   %this.selectedCorner = !%this.selectedCorner;
}

function NDM_PlantCopy::onSelectObject(%this, %client, %obj, %pos, %normal)
{
   %this.moveBricksTo(%client, %pos, %normal);
}

function NDM_PlantCopy::onCancelBrick(%this, %client) { %client.ndSetMode(%client.ndLastSelectMode); }
function NDM_PlantCopy::getBottomPrint(%this, %client)
{
   return ndFormatMessage("Plant Mode", "", "");
}

function ND_Selection::finishStackSelection(%this)
{
   %msg = "Selected " @ %this.brickCount @ " (Limit Reached) " @ %this.trustFailCount @ " missing trust";
   commandToClient(%this.client, 'centerPrint', %msg, 5);
}

function ND_Selection::finishBoxSelection(%this)
{
   commandToClient(%this.client, 'centerPrint', "Press [Cancel Brick] to adjust the box.", 8);
}

function ND_Selection::finishPlant(%this)
{
   commandToClient(%this.client, 'centerPrint', "Planted, some blocked. Some floating.", 4);
}

function ND_Selection::finishCutting(%this)
{
   %msg = "<font:Verdana:20>\c6Cut \c3" @ %this.cutSuccessCount;
   commandToClient(%this.client, 'centerPrint', %msg, 8);
}

function serverCmdMirrorX(%client) { %client.ndMirror(0); }
function serverCmdMirrorY(%client) { %client.ndMirror(1); }
function serverCmdCut(%client) { serverCmdNdCut(%client); }

function serverCmdSaveDup(%client, %fileName)
{
   if(%client.ndLastSaveTime + 10 > $Sim::Time)
      return;
   if($Pref::Server::ND::SaveAdminOnly && !%client.isAdmin)
      return;
   if(strLen(%fileName) > 20)
      return;
}

function serverCmdLoadDup(%client, %fileName)
{
   if(%client.ndLastLoadTime + 5 > $Sim::Time)
      return;
   if($Pref::Server::ND::LoadAdminOnly && !%client.isAdmin)
      return;
}

function ND_Selection::finishSaving(%this)
{
   messageClient(%this.client, '', "Finished saving selection");
}

function ND_Selection::finishLoading(%this)
{
   messageClient(%this.client, '', "Finished loading selection");
}

function serverCmdDupHelp(%client)
{
   messageClient(%client, '', "You can use the following commands:");
}

function serverCmdNdMultiSelect(%client, %bool) { %client.ndMultiSelect = !!%bool; }
function NDM_StackSelect::onShiftBrick(%this, %client, %x, %y, %z) { %client.ndSetMode(NDM_PlantCopy); }
function NDM_StackSelect::onPlantBrick(%this, %client) { %client.ndSetMode(NDM_PlantCopy); }

function NDM_BoxSelect::onStartMode(%this, %client, %lastMode)
{
   %min = %client.ndSelection.minSize;
   %max = %client.ndSelection.maxSize;
   %client.ndSelectionBox.setSizeAligned(%min, %max, %client.player);
}

function ND_SelectionBox::shift(%this, %offset)
{
   %this.point1 = vectorAdd(%this.point1, %offset);
   %this.point2 = vectorAdd(%this.point2, %offset);
}

function ND_SelectionBox::rotate(%this, %direction)
{
   %this.point1 = ndRotateVector(%this.point1, %direction);
}

function serverCmdMirrorZ(%client) { %client.ndMirror(2); }

function serverCmdForcePlant(%client)
{
   if($Pref::Server::ND::FloatAdminOnly && !%client.isAdmin)
      return;
   NDM_PlantCopy.conditionalPlant(%client, true);
}

function serverCmdToggleForcePlant(%client) { %client.ndForcePlant = !%client.ndForcePlant; }

package NewDuplicator_Stand_In_Paint
{
   function serverCmdUseSprayCan(%client, %index) { %client.ndSetMode(NDM_FillColor); }
   function serverCmdUseFxCan(%client, %index) { %client.ndSetMode(NDM_FillColor); }
};
activatePackage(NewDuplicator_Stand_In_Paint);

function NDM_FillColor::onPlantBrick(%this, %client)
{
   if($Pref::Server::ND::PaintAdminOnly || $Pref::Server::ND::PaintFxAdminOnly)
      return;
   %client.ndSelection.startFillColor(0, %client.currentColor);
}

function NDM_FillColor::onChangeMode(%this, %client) { commandToClient(%client, 'setScrollMode', 2); }
function ND_Selection::finishFillColor(%this) { commandToClient(%this.client, 'centerPrint', "Painted", 8); }

function serverCmdFillWrench(%client)
{
   if($Pref::Server::ND::WrenchAdminOnly && !%client.isAdmin)
      return;
   commandToClient(%client, 'ndOpenWrenchGui');
}

function ND_Selection::finishFillWrench(%this) { commandToClient(%this.client, 'centerPrint', "Applied changes to", 8); }
function serverCmdSuperCut(%client) { commandToClient(%client, 'messageBoxOkCancel', "Supercut", "Sure?", 'ndConfirmSuperCut'); }
function serverCmdNdConfirmSuperCut(%client) { %client.ndMode.onSuperCut(%client); }

function ND_Selection::finishSuperCut(%this)
{
   commandToClient(%this.client, 'centerPrint', "Deleted, placed a new one", 12);
   %this.client.doFillBricks();
}

function serverCmdFillBricks(%client)
{
   if($Pref::Server::ND::FillBricksAdminOnly && !%client.isAdmin)
      return;
   commandToClient(%client, 'messageBoxOkCancel', "Fill", "Sure?", 'ndConfirmFillBricks');
}

function serverCmdNdConfirmFillBricks(%client) { %client.fillBricksAfterSuperCut = true; }

function GameConnection::doFillBricks(%this)
{
   $ND::FillBrickColorID = %this.currentColor;
   messageClient(%this, '', "Filled in");
}
