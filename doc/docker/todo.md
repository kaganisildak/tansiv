 - In order to simulate actual ethernet links, the qemu version of TANSIV pads the 1514 bytes packets to add what would be used for the CRC and inter-framepadding.
   This is supposed to give better precision, but is not done in the current implementation of tandocker.
