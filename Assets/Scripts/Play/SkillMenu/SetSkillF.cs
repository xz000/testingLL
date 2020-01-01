using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillF : MonoBehaviour
{
    public Toggle F1;
    public Image IconF;
    GameObject Soldier;

    public void SetF()
    {
        GetComponent<SkillsLink>().KeyFSkill = null;
        if (F1.isOn)
        {
            GetComponent<SkillsLink>().KeyFSkill = SkillCode.TestSkill03;
            return;
        }
    }
}
