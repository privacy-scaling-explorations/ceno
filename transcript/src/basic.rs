use ff_ext::ExtensionField;
use goldilocks::SmallField;
use poseidon::Poseidon;

use crate::Challenge;

// temporarily using 12-4 hashes
pub const INPUT_WIDTH: usize = 12;
pub const OUTPUT_WIDTH: usize = 4;

#[derive(Clone)]
pub struct Transcript<E: ExtensionField> {
    sponge_hasher: Poseidon<E::BaseField, 12, 11>,
}

impl<E: ExtensionField> Transcript<E> {
    /// Create a new IOP transcript.
    pub fn new(label: &'static [u8]) -> Self {
        // FIXME: change me the the right parameter
        let mut hasher = Poseidon::<E::BaseField, _, _>::new(8, 22);
        let label_f = E::BaseField::bytes_to_field_elements(label);
        hasher.update(label_f.as_slice());
        Self {
            sponge_hasher: hasher,
        }
    }
}

impl<E: ExtensionField> Transcript<E> {
    // Append the message to the transcript.
    pub fn append_message(&mut self, msg: &[u8]) {
        let msg_f = E::BaseField::bytes_to_field_elements(msg);
        self.sponge_hasher.update(&msg_f);
    }

    // Append the field extension element to the transcript.
    pub fn append_field_element_ext(&mut self, element: &E) {
        self.sponge_hasher.update(element.as_bases());
    }

    pub fn append_field_element_exts(&mut self, element: &[E]) {
        for e in element {
            self.append_field_element_ext(e);
        }
    }

    // Append the field elemetn to the transcript.
    pub fn append_field_element(&mut self, element: &E::BaseField) {
        self.sponge_hasher.update(&[*element]);
    }

    // Append the challenge to the transcript.
    pub fn append_challenge(&mut self, challenge: Challenge<E>) {
        self.sponge_hasher.update(challenge.elements.as_bases())
    }

    // // Append the message to the transcript.
    // pub fn append_serializable_element<S: Serialize>(
    //     &mut self,
    //     _label: &'static [u8],
    //     _element: &S,
    // ) {
    //     unimplemented!()
    // }

    // Generate the challenge from the current transcript
    // and append it to the transcript.
    //
    // The output field element is statistical uniform as long
    // as the field has a size less than 2^384.
    pub fn get_and_append_challenge(&mut self, label: &'static [u8]) -> Challenge<E> {
        self.append_message(label);

        let challenge = Challenge {
            elements: E::from_limbs(self.sponge_hasher.squeeze_vec().as_ref()),
        };
        challenge
    }

    pub fn commit_rolling(&mut self) {
        // do nothing
    }

    pub fn read_field_element_ext(&self) -> E {
        unimplemented!()
    }

    pub fn read_field_element_exts(&self) -> Vec<E> {
        unimplemented!()
    }

    pub fn read_field_element(&self) -> <E as ExtensionField>::BaseField {
        unimplemented!()
    }

    pub fn read_challenge(&self, _challenge: Challenge<E>) {
        unimplemented!()
    }

    pub fn send_challenge(&self, _challenge: E) {
        unimplemented!()
    }
}