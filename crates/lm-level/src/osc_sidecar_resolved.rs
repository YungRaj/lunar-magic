use crate::{OscDirective, OscDisplayTile, OscObjectSelector, OscSidecar};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OscResolvedObject {
    pub selector: OscObjectSelector,
    pub description: Option<String>,
    pub display: Option<Vec<OscDisplayTile>>,
    pub values: Option<Vec<[u16; 8]>>,
    pub attributes: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OscResolvedTable {
    objects: Vec<OscResolvedObject>,
}

impl OscResolvedTable {
    #[must_use]
    pub fn from_sidecar(sidecar: &OscSidecar) -> Self {
        let mut result = Self {
            objects: Vec::new(),
        };
        for entry in sidecar.entries() {
            for selector in &entry.selectors {
                let target = result.object_mut(*selector);
                match &entry.directive {
                    OscDirective::Description(value) => target.description = Some(value.clone()),
                    OscDirective::Display(value) => target.display = Some(value.clone()),
                    OscDirective::Values(value) => target.values = Some(value.clone()),
                    OscDirective::Attributes(value) => target.attributes = Some(value.clone()),
                }
            }
        }
        result
    }

    #[must_use]
    pub fn objects(&self) -> &[OscResolvedObject] {
        &self.objects
    }

    #[must_use]
    pub fn get(&self, selector: OscObjectSelector) -> Option<&OscResolvedObject> {
        self.objects.iter().find(|entry| entry.selector == selector)
    }

    /// Resolves the first default display for an object's family, parameter, and tileset variant.
    #[must_use]
    pub fn default_display(
        &self,
        object_type: u8,
        parameter: u8,
        variant: u8,
    ) -> Option<&OscResolvedObject> {
        self.objects.iter().find(|entry| {
            entry.selector.object_type == object_type
                && entry.selector.parameter == parameter
                && entry.selector.variant == variant
                && entry.display.is_some()
        })
    }

    fn object_mut(&mut self, selector: OscObjectSelector) -> &mut OscResolvedObject {
        if let Some(index) = self
            .objects
            .iter()
            .position(|entry| entry.selector == selector)
        {
            return &mut self.objects[index];
        }
        self.objects.push(OscResolvedObject {
            selector,
            description: None,
            display: None,
            values: None,
            attributes: None,
        });
        self.objects.last_mut().expect("just inserted")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_order_replaces_only_the_matching_domain() {
        let source = OscSidecar::decode(
            b"10\t2\t13\t0,0,10\n10\t2\t19\t1,2,3,4,5,6,7,8\n10\t2\t13\t8,9,11\n",
        )
        .unwrap();
        let table = OscResolvedTable::from_sidecar(&source);
        let value = table.default_display(0x10, 2, 1).unwrap();
        assert_eq!(value.display.as_ref().unwrap()[0].tile, 0x11);
        assert_eq!(value.values.as_ref().unwrap()[0], [1, 2, 3, 4, 5, 6, 7, 8]);
    }
}
