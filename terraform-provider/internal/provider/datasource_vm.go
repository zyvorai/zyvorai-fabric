// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework/datasource"
	"github.com/hashicorp/terraform-plugin-framework/datasource/schema"
	"github.com/hashicorp/terraform-plugin-framework/types"
)

var _ datasource.DataSource = &vmDataSource{}

type vmDataSource struct {
	client *Client
}

type vmDataSourceModel struct {
	Name   types.String `tfsdk:"name"`
	State  types.String `tfsdk:"state"`
	CPUs   types.Int64  `tfsdk:"cpus"`
	Memory types.Int64  `tfsdk:"memory"`
	Image  types.String `tfsdk:"image"`
	IP     types.String `tfsdk:"ip_address"`
}

func NewVMDataSource() datasource.DataSource {
	return &vmDataSource{}
}

func (d *vmDataSource) Metadata(_ context.Context, req datasource.MetadataRequest, resp *datasource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_vm"
}

func (d *vmDataSource) Schema(_ context.Context, _ datasource.SchemaRequest, resp *datasource.SchemaResponse) {
	resp.Schema = schema.Schema{
		Description: "Reads an existing Zyvor Fabric VM.",
		Attributes: map[string]schema.Attribute{
			"name": schema.StringAttribute{
				Required:    true,
				Description: "VM name",
			},
			"state":  schema.StringAttribute{Computed: true},
			"cpus":   schema.Int64Attribute{Computed: true},
			"memory": schema.Int64Attribute{Computed: true},
			"image":  schema.StringAttribute{Computed: true},
			"ip_address": schema.StringAttribute{Computed: true},
		},
	}
}

func (d *vmDataSource) Configure(_ context.Context, req datasource.ConfigureRequest, resp *datasource.ConfigureResponse) {
	if req.ProviderData == nil {
		return
	}
	client, ok := req.ProviderData.(*Client)
	if !ok {
		return
	}
	d.client = client
}

func (d *vmDataSource) Read(ctx context.Context, req datasource.ReadRequest, resp *datasource.ReadResponse) {
	var config vmDataSourceModel
	resp.Diagnostics.Append(req.Config.Get(ctx, &config)...)
	if resp.Diagnostics.HasError() {
		return
	}

	vm, err := d.client.GetVM(ctx, config.Name.ValueString())
	if err != nil {
		resp.Diagnostics.AddError("Read VM failed", err.Error())
		return
	}

	config.State = types.StringValue(vm.State)
	config.CPUs = types.Int64Value(vm.CPUs)
	config.Memory = types.Int64Value(vm.Memory)
	config.Image = types.StringValue(vm.Image)
	if vm.IP != nil {
		config.IP = types.StringValue(*vm.IP)
	} else {
		config.IP = types.StringNull()
	}
	resp.Diagnostics.Append(resp.State.Set(ctx, &config)...)
}
