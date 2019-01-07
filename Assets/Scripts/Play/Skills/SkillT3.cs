using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillT3 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    //public float maxdistance;
    public float bulletspeed = 20;
    public GameObject fireball;
    public float damage = 5;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void Update()
    {
        if (Input.GetButtonDown("FireT") && skillavaliable)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            //MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    public void Skill(Fix64Vector2 actionplacef)
    {
        Vector2 actionplace = actionplacef.ToV2();
        GetComponent<DoSkill>().BeforeSkill();
        Vector2 singplace = transform.position;
        Vector2 skilldirection = actionplace - singplace;
        DoFire(singplace + 0.81f * skilldirection.normalized, skilldirection.normalized * bulletspeed);
        currentcooldown = 0;
        skillavaliable = false;
    }

    //[PunRPC]
    void DoFire(Vector2 fireplace, Vector2 speed2d)
    {
        GameObject bullet;
        fireball.GetComponent<JumpBulletScript>().sender = gameObject;
        bullet = Instantiate(fireball, fireplace, Quaternion.identity);
        bullet.GetComponent<JumpBulletScript>().Damage = damage;
        bullet.GetComponent<Rigidbody2D>().velocity = speed2d;
    }
}
