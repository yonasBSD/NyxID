import { useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import {
  createApiKeySchema,
  type CreateApiKeyFormData,
  API_KEY_SCOPES,
} from "@/schemas/api-keys";
import { useCreateApiKey } from "@/hooks/use-api-keys";
import { useNodes } from "@/hooks/use-nodes";
import { copyToClipboard } from "@/lib/utils";
import { ApiError } from "@/lib/api-client";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Plus, Copy, Check } from "lucide-react";
import { toast } from "sonner";

function toggleInArray(
  items: readonly string[],
  item: string,
): readonly string[] {
  return items.includes(item)
    ? items.filter((i) => i !== item)
    : [...items, item];
}

export function ApiKeyCreateDialog() {
  const [open, setOpen] = useState(false);
  const [createdKey, setCreatedKey] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const createMutation = useCreateApiKey();

  const { data: nodes } = useNodes();

  const form = useForm<CreateApiKeyFormData>({
    resolver: zodResolver(createApiKeySchema),
    defaultValues: {
      name: "",
      scopes: [],
      expires_at: null,
      description: null,
      allow_all_services: true,
      allow_all_nodes: true,
      allowed_service_ids: [],
      allowed_node_ids: [],
      callback_url: null,
    },
  });

  const watchAllNodes = form.watch("allow_all_nodes") ?? true;

  async function onSubmit(data: CreateApiKeyFormData) {
    try {
      const result = await createMutation.mutateAsync(data);
      setCreatedKey(result.full_key);
      toast.success("API key created successfully");
    } catch (error) {
      if (error instanceof ApiError) {
        form.setError("root", { message: error.message });
      } else {
        toast.error("Failed to create API key");
      }
    }
  }

  async function handleCopy() {
    if (!createdKey) return;
    await copyToClipboard(createdKey);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  function handleClose() {
    setOpen(false);
    setCreatedKey(null);
    setCopied(false);
    form.reset();
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => (o ? setOpen(true) : handleClose())}
    >
      <DialogTrigger asChild>
        <Button>
          <Plus className="mr-2 h-4 w-4" />
          Create API Key
        </Button>
      </DialogTrigger>
      <DialogContent className="max-h-[85vh] overflow-y-auto">
        {createdKey ? (
          <>
            <DialogHeader>
              <DialogTitle>API Key Created</DialogTitle>
              <DialogDescription>
                Copy your API key now. You will not be able to see it again.
              </DialogDescription>
            </DialogHeader>
            <div className="flex items-center gap-2">
              <code className="flex-1 rounded-md bg-muted p-3 font-mono text-sm break-all">
                {createdKey}
              </code>
              <Button
                variant="outline"
                size="icon"
                onClick={() => void handleCopy()}
              >
                {copied ? (
                  <Check className="h-4 w-4 text-success" />
                ) : (
                  <Copy className="h-4 w-4" />
                )}
              </Button>
            </div>
            <DialogFooter>
              <Button onClick={handleClose}>Done</Button>
            </DialogFooter>
          </>
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>Create API Key</DialogTitle>
              <DialogDescription>
                Create a new API key to access the NyxID API.
              </DialogDescription>
            </DialogHeader>

            <Form {...form}>
              <form
                onSubmit={form.handleSubmit(onSubmit)}
                className="space-y-4"
              >
                {form.formState.errors.root && (
                  <div className="rounded-md bg-destructive/10 p-3 text-sm text-destructive">
                    {form.formState.errors.root.message}
                  </div>
                )}

                <FormField
                  control={form.control}
                  name="name"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Name</FormLabel>
                      <FormControl>
                        <Input placeholder="My API Key" {...field} />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                <FormField
                  control={form.control}
                  name="scopes"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Scopes</FormLabel>
                      <div className="flex flex-wrap gap-2">
                        {API_KEY_SCOPES.map((scope) => {
                          const isSelected = (
                            field.value as readonly string[]
                          ).includes(scope);
                          return (
                            <Badge
                              key={scope}
                              variant={isSelected ? "default" : "outline"}
                              className="cursor-pointer"
                              onClick={() =>
                                field.onChange(
                                  toggleInArray(
                                    field.value as readonly string[],
                                    scope,
                                  ),
                                )
                              }
                            >
                              {scope}
                            </Badge>
                          );
                        })}
                      </div>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                <FormField
                  control={form.control}
                  name="expires_at"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>
                        Expiry Date{" "}
                        <span className="text-muted-foreground">
                          (optional)
                        </span>
                      </FormLabel>
                      <FormControl>
                        <Input
                          type="date"
                          min={new Date().toISOString().slice(0, 10)}
                          {...field}
                          value={field.value ?? ""}
                          onChange={(e) =>
                            field.onChange(e.target.value || null)
                          }
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                <FormField
                  control={form.control}
                  name="callback_url"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>
                        Callback URL{" "}
                        <span className="text-muted-foreground">
                          (optional)
                        </span>
                      </FormLabel>
                      <FormControl>
                        <Input
                          type="url"
                          placeholder="https://my-agent.example.com/webhook"
                          {...field}
                          value={field.value ?? ""}
                          onChange={(e) =>
                            field.onChange(e.target.value || null)
                          }
                        />
                      </FormControl>
                      <p className="text-xs text-muted-foreground">
                        Where NyxID sends channel relay messages. Required for Channel Bot routing.
                      </p>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                {/* Node scope */}
                <FormField
                  control={form.control}
                  name="allow_all_nodes"
                  render={({ field }) => (
                    <FormItem>
                      <div className="flex items-center gap-2">
                        <Checkbox
                          id="allow-all-nodes"
                          checked={field.value}
                          onCheckedChange={(checked) =>
                            field.onChange(checked === true)
                          }
                        />
                        <Label
                          htmlFor="allow-all-nodes"
                          className="text-sm font-medium"
                        >
                          Allow all nodes
                        </Label>
                      </div>
                    </FormItem>
                  )}
                />

                {!watchAllNodes && (
                  <FormField
                    control={form.control}
                    name="allowed_node_ids"
                    render={({ field }) => (
                      <FormItem>
                        <div className="space-y-2 rounded-lg border border-border p-3">
                          <p className="text-xs text-muted-foreground">
                            Restrict to specific nodes:
                          </p>
                          {nodes && nodes.length > 0 ? (
                            nodes.map((n) => (
                              <div
                                key={n.id}
                                className="flex items-center gap-2"
                              >
                                <Checkbox
                                  id={`create-node-${n.id}`}
                                  checked={(
                                    field.value as readonly string[]
                                  ).includes(n.id)}
                                  onCheckedChange={() =>
                                    field.onChange(
                                      toggleInArray(
                                        field.value as readonly string[],
                                        n.id,
                                      ),
                                    )
                                  }
                                />
                                <Label
                                  htmlFor={`create-node-${n.id}`}
                                  className="text-xs"
                                >
                                  {n.name}
                                  <Badge
                                    variant={
                                      n.status === "Online"
                                        ? "default"
                                        : "secondary"
                                    }
                                    className="ml-1 text-[10px]"
                                  >
                                    {n.status}
                                  </Badge>
                                </Label>
                              </div>
                            ))
                          ) : (
                            <p className="text-xs text-muted-foreground">
                              No nodes registered yet.
                            </p>
                          )}
                        </div>
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                )}

                <DialogFooter>
                  <Button type="button" variant="outline" onClick={handleClose}>
                    Cancel
                  </Button>
                  <Button type="submit" isLoading={createMutation.isPending}>
                    Create key
                  </Button>
                </DialogFooter>
              </form>
            </Form>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
