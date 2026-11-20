// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

/// Generate an OVF XML descriptor for a virtual machine.
pub fn generate_ovf(name: &str, cpus: u32, memory_mb: u64, disk_size: u64) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Envelope xmlns="http://schemas.dmtf.org/ovf/envelope/1"
          xmlns:rasd="http://schemas.dmtf.org/wbem/wscim/1/cim-schema/2/CIM_ResourceAllocationSettingData"
          xmlns:vssd="http://schemas.dmtf.org/wbem/wscim/1/cim-schema/2/CIM_VirtualSystemSettingData">
  <References>
    <File ovf:id="disk1" ovf:href="{name}.vmdk" ovf:size="{disk_size}" xmlns:ovf="http://schemas.dmtf.org/ovf/envelope/1"/>
  </References>
  <DiskSection>
    <Info>Virtual disk information</Info>
    <Disk ovf:diskId="vmdisk1" ovf:capacity="{disk_size}" ovf:fileRef="disk1" ovf:format="http://www.vmware.com/interfaces/specifications/vmdk.html#streamOptimized" xmlns:ovf="http://schemas.dmtf.org/ovf/envelope/1"/>
  </DiskSection>
  <VirtualSystem ovf:id="{name}" xmlns:ovf="http://schemas.dmtf.org/ovf/envelope/1">
    <Info>A virtual machine</Info>
    <Name>{name}</Name>
    <VirtualHardwareSection>
      <Info>Virtual hardware requirements</Info>
      <System>
        <vssd:ElementName>Virtual Hardware Family</vssd:ElementName>
        <vssd:InstanceID>0</vssd:InstanceID>
        <vssd:VirtualSystemType>vmx-14</vssd:VirtualSystemType>
      </System>
      <Item>
        <rasd:Description>Number of virtual CPUs</rasd:Description>
        <rasd:ElementName>{cpus} virtual CPU(s)</rasd:ElementName>
        <rasd:InstanceID>1</rasd:InstanceID>
        <rasd:ResourceType>3</rasd:ResourceType>
        <rasd:VirtualQuantity>{cpus}</rasd:VirtualQuantity>
      </Item>
      <Item>
        <rasd:Description>Memory Size</rasd:Description>
        <rasd:ElementName>{memory_mb}MB of memory</rasd:ElementName>
        <rasd:InstanceID>2</rasd:InstanceID>
        <rasd:ResourceType>4</rasd:ResourceType>
        <rasd:VirtualQuantity>{memory_mb}</rasd:VirtualQuantity>
      </Item>
    </VirtualHardwareSection>
  </VirtualSystem>
</Envelope>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ovf() {
        let ovf = generate_ovf("test-vm", 2, 2048, 10737418240);
        assert!(ovf.contains("<Name>test-vm</Name>"));
        assert!(ovf.contains("2 virtual CPU(s)"));
        assert!(ovf.contains("2048MB of memory"));
        assert!(ovf.contains("test-vm.vmdk"));
    }
}
