using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class CooldownImage : MonoBehaviour
{
    public Image Icon;
    public delegate float delef();
    public delef Fif;

    // Use this for initialization
    void Start()
    {
        Icon = gameObject.GetComponent<Image>();
        //enabled = false;
    }

    // Update is called once per frame
    void Update()
    {
        Icon.fillAmount = Fif();
    }

    private void OnDisable()
    {
        Icon.fillAmount = 1;
    }
}
