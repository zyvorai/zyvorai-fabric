// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework/path"
	"github.com/hashicorp/terraform-plugin-framework/resource"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/int64planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/stringplanmodifier"
	"github.com/hashicorp/terraform-plugin-framework/types"
)

var _ resource.Resource = &vmResource{}

type vmResource struct {
	client *Client
}

type vmResourceModel struct {
	Name   types.String `tfsdk:"name"`
	Image  types.String `tfsdk:"image"`
	CPUs   types.Int64  `tfsdk:"cpus"`
	Memory types.Int64  `tfsdk:"memory"`
	State  types.String `tfsdk:"state"`
	IP     types.String `tfsdk:"ip_address"`
}

func NewVMResource() resource.Resource {
	return &vmResource{}
}

func (r *vmResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_vm"
}

func (r *vmResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	resp.Schema = schema.Schema{
		Description: "Manages a Zyvor Fabric virtual machine.",
		Attributes: map[string]schema.Attribute{
			"name": schema.StringAttribute{
				Required:    true,
				Description: "VM name",
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"image": schema.StringAttribute{
				Required:    true,
				Description: "Disk image path on the hypervisor",
			},
			"cpus": schema.Int64Attribute{
				Optional:    true,
				Computed:    true,
				Description: "vCPU count",
				PlanModifiers: []planmodifier.Int64{
					int64planmodifier.UseStateForUnknown(),
				},
			},
			"memory": schema.Int64Attribute{
				Optional:    true,
				Computed:    true,
				Description: "Memory in MB",
				PlanModifiers: []planmodifier.Int64{
					int64planmodifier.UseStateForUnknown(),
				},
			},
			"state": schema.StringAttribute{
				Computed:    true,
				Description: "Current VM state",
			},
			"ip_address": schema.StringAttribute{
				Computed:    true,
				Description: "Primary IP if known",
			},
		},
	}
}

func (r *vmResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
	if req.ProviderData == nil {
		return
	}
	client, ok := req.ProviderData.(*Client)
	if !ok {
		resp.Diagnostics.AddError("Invalid provider data", "expected *Client")
		return
	}
	r.client = client
}

func (r *vmResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan vmResourceModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}

	client := r.client
	cpus := plan.CPUs.ValueInt64()
	if cpus == 0 {
		cpus = 2
	}
	memory := plan.Memory.ValueInt64()
	if memory == 0 {
		memory = 2048
	}

	vm, err := client.CreateVM(ctx, createVMRequest{
		Name:   plan.Name.ValueString(),
		Image:  plan.Image.ValueString(),
		CPUs:   cpus,
		Memory: memory,
	})
	if err != nil {
		resp.Diagnostics.AddError("Create VM failed", err.Error())
		return
	}

	_ = client.StartVM(ctx, plan.Name.ValueString())
	if refreshed, err := client.GetVM(ctx, plan.Name.ValueString()); err == nil {
		vm = refreshed
	}

	plan.CPUs = types.Int64Value(vm.CPUs)
	plan.Memory = types.Int64Value(vm.Memory)
	plan.State = types.StringValue(vm.State)
	if vm.IP != nil {
		plan.IP = types.StringValue(*vm.IP)
	} else {
		plan.IP = types.StringNull()
	}
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *vmResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	var state vmResourceModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}

	client := r.client
	vm, err := client.GetVM(ctx, state.Name.ValueString())
	if err != nil {
		if IsNotFound(err) {
			resp.State.RemoveResource(ctx)
			return
		}
		resp.Diagnostics.AddError("Read failed", err.Error())
		return
	}

	state.CPUs = types.Int64Value(vm.CPUs)
	state.Memory = types.Int64Value(vm.Memory)
	state.State = types.StringValue(vm.State)
	state.Image = types.StringValue(vm.Image)
	if vm.IP != nil {
		state.IP = types.StringValue(*vm.IP)
	} else {
		state.IP = types.StringNull()
	}
	resp.Diagnostics.Append(resp.State.Set(ctx, &state)...)
}

func (r *vmResource) Update(ctx context.Context, req resource.UpdateRequest, resp *resource.UpdateResponse) {
	resp.Diagnostics.AddError("Update not supported", "Recreate the VM to change image, cpus, or memory")
}

func (r *vmResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state vmResourceModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	client := r.client
	if err := client.DeleteVM(ctx, state.Name.ValueString()); err != nil {
		resp.Diagnostics.AddError("Delete VM failed", err.Error())
	}
}

func (r *vmResource) ImportState(ctx context.Context, req resource.ImportStateRequest, resp *resource.ImportStateResponse) {
	resource.ImportStatePassthroughID(ctx, path.Root("name"), req, resp)
}
