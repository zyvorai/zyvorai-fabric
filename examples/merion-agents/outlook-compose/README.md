# outlook-compose

Writes a plain-text mail as an Outlook **draft** (the default) or **sends** it. Both wait for the person: the credentials need a decision signed by their phone key, and the approval shows the recipients, subject and text as the host read them out of the Graph request ([what the person sees](../../../docs/merion/connectors/README.md#what-the-person-sees-before-they-approve)).

Same chat form as [mail-compose](../mail-compose/README.md): `draft` or `send`, `to:`, `subject:`, a blank line, the text. It refuses, before any request, anything that is not a plain address, more than ten recipients, a multi-line subject, and an empty or over-long text. Not run against real Microsoft.
